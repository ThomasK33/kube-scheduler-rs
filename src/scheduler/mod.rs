mod algorithms;
mod filters;

use std::time::{Duration, Instant};

use color_eyre::Result;
use futures::{StreamExt, TryStreamExt};
use k8s_openapi::api::core::v1::Pod;
use kube::{
    api::ListParams,
    core::ObjectList,
    runtime::{reflector, watcher, WatchStreamExt},
    Api, Client,
};

use k8s_openapi::{api::core::v1::Binding, apimachinery::pkg::apis::meta::v1::Status};
use kube::api::PostParams;

use crate::Cli;

pub(crate) struct SchedulingParameters {
    pub client: Client,
    pub scheduler_name: String,
    pub unscheduled_pods: ObjectList<Pod>,
}

pub(crate) async fn run_scheduler(cli: Cli) -> Result<()> {
    // Infer the runtime environment and try to create a Kubernetes Client
    let client = Client::try_default().await?;

    let pods: Api<Pod> = Api::all(client.clone());

    // List params to only obtain pods that are unscheduled/not bound to a node and
    // has the specified scheduler name set
    let unscheduled_lp = ListParams::default()
        .fields(format!("spec.schedulerName={},spec.nodeName=", cli.scheduler_name).as_str());

    let (_, pod_writer) = reflector::store();
    let pod_reflector = reflector(pod_writer, watcher(pods.clone(), unscheduled_lp.clone()));

    let mut handle: tokio::task::JoinHandle<Result<()>> = tokio::spawn(async { Ok(()) });

    log::info!("Running reflector loop");

    let mut last_run = Instant::now();

    let mut pod_reflector = pod_reflector.applied_objects().boxed();
    while (pod_reflector.try_next().await?).is_some() {
        let client = client.clone();
        let pods = pods.clone();
        let scheduler_name = cli.scheduler_name.clone();
        let unscheduled_lp = unscheduled_lp.clone();

        // A timeout, after which a scheduler run is triggered anyways
        if last_run.elapsed() < Duration::from_secs(cli.debounce_duration) {
            // Abort previous handle
            handle.abort();
        } else {
            // Wait for the previous handle to finish ignore the result
            let _ = handle.await;
            // Update last run time
            last_run = Instant::now();
        }

        // Spawn a new handle
        handle = tokio::spawn(async move {
            // Debounce schedule invocation by one second after the last invocation of this
            // loop
            tokio::time::sleep(Duration::from_secs(cli.debounce_duration)).await;

            let params = SchedulingParameters {
                client: client.clone(),
                scheduler_name,
                unscheduled_pods: pods.list(&unscheduled_lp).await?,
            };

            if params.unscheduled_pods.items.len() == 0 {
                return Err(color_eyre::eyre::eyre!(
                    "No unscheduled pods found after debouncing"
                ));
            }

            match cli.algorithm {
                crate::Algorithm::BinPacking => algorithms::bin_packing::schedule(params).await,
            }
        });
    }

    Ok(())
}

// Util functions shared by multiple scheduler algorithms

pub(crate) struct PodBindParameters {
    pub(crate) client: Client,
    pub(crate) pod_name: String,
    pub(crate) pod_namespace: String,
    pub(crate) node_name: String,
    pub(crate) scheduler_name: String,
}

pub(crate) async fn bind_pod_to_node(params: PodBindParameters) -> Result<()> {
    let PodBindParameters {
        client,
        pod_name,
        pod_namespace,
        node_name,
        scheduler_name,
    } = params;

    let pods: Api<Pod> = Api::namespaced(client.clone(), &pod_namespace);

    let res: Result<Status, kube::Error> = pods
        .create_subresource(
            "binding",
            &pod_name.clone(),
            &PostParams {
                field_manager: Some(scheduler_name.clone()),
                ..Default::default()
            },
            serde_json::to_vec(&Binding {
                metadata: kube::core::ObjectMeta {
                    name: Some(pod_name.clone()),
                    ..Default::default()
                },
                target: k8s_openapi::api::core::v1::ObjectReference {
                    api_version: Some("v1".to_owned()),
                    kind: Some("Node".to_owned()),
                    name: Some(node_name.clone()),
                    ..Default::default()
                },
            })?,
        )
        .await;
    log::debug!("res: {res:#?}");

    let status = res?;
    let Some(code) =  status.code else { color_eyre::eyre::bail!("Could not obtain status code from kubernetes response") };

    if code >= 200 && code <= 202 {
        Ok(())
    } else {
        Err(color_eyre::eyre::eyre!(
            "An error occurred while trying to bind pod to node: {status:?}"
        ))
    }
}
