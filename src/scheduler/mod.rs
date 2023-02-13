mod algorithms;

use color_eyre::Result;
use futures::{StreamExt, TryStreamExt};
use k8s_openapi::api::core::v1::{Node, Pod};
use kube::{
    api::ListParams,
    core::ObjectList,
    runtime::{reflector, watcher, WatchStreamExt},
    Api, Client,
};

use crate::Cli;

pub(crate) struct SchedulingParameters {
    pub scheduler_name: String,

    pub all_nodes: ObjectList<Node>,
    // pub all_pods: ObjectList<Pod>,
    pub unscheduled_pods: ObjectList<Pod>,

    pub client: Client,
}

pub(crate) async fn run_scheduler(cli: Cli) -> Result<()> {
    // Infer the runtime environment and try to create a Kubernetes Client
    let client = Client::try_default().await?;

    let nodes: Api<Node> = Api::all(client.clone());
    let pods: Api<Pod> = Api::all(client.clone());

    // List params to only obtain pods that are unscheduled/not bound to a node and
    // has the specified scheduler name set
    let unscheduled_lp = ListParams::default()
        .fields(format!("spec.schedulerName={},spec.nodeName=", cli.scheduler_name).as_str());

    let (_, pod_writer) = reflector::store();
    let pod_reflector = reflector(pod_writer, watcher(pods.clone(), unscheduled_lp.clone()));

    let mut handle: tokio::task::JoinHandle<Result<()>> = tokio::spawn(async { Ok(()) });

    log::info!("Running reflector loop");

    let mut pod_reflector = pod_reflector.applied_objects().boxed();
    while (pod_reflector.try_next().await?).is_some() {
        let params = SchedulingParameters {
            all_nodes: nodes.list(&ListParams::default()).await?,
            // all_pods: pods.list(&ListParams::default()).await?,
            unscheduled_pods: pods.list(&unscheduled_lp).await?,
            client: client.clone(),
            scheduler_name: cli.scheduler_name.clone(),
        };

        // Abort previous handle
        handle.abort();
        handle = tokio::spawn(async move {
            // Debounce schedule invocation by one second after the last invocation of this loop
            tokio::time::sleep(std::time::Duration::from_secs(cli.debounce_duration)).await;

            match cli.algorithm {
                crate::Algorithm::BinPacking => algorithms::bin_packing::schedule(params).await,
            }
        });
    }

    Ok(())
}
