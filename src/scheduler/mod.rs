mod algorithms;
mod filters;

use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

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
    pub client: Client,
    pub scheduler_name: String,
    pub unscheduled_pods: ObjectList<Pod>,
}

// Schedule state used to keep track of the current state of the cluster.
pub(crate) struct WorldState {
    // Current nodes in the cluster
    pub(crate) nodes: ObjectList<Node>,
    // Pods that are not bound to a node
    pub(crate) unscheduled_pods: ObjectList<Pod>,
    // State of the node to pod task scheduling
    pub(crate) state: BTreeMap<String, ObjectList<Pod>>,
}

pub(crate) enum Reason {
    NoFeasibleNode,
    NodeName,
    NodePods,
}

// Will be used by the reconciler to change pod node bindings and perform preemption
pub(crate) struct TargetState {
    // Pods that are not bound to a node
    pub(crate) unscheduled_pods: Vec<(Pod, Reason)>,
    // State of the node to pod task scheduling
    pub(crate) state: BTreeMap<String, Vec<Pod>>,
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

            let nodes: ObjectList<Node> = Api::all(client.clone())
                // TODO: might potentially add some filtering here in the future
                .list(&ListParams::default())
                .await?;

            // Get a mapping of nodes and their pods
            let state: BTreeMap<String, ObjectList<Pod>> = {
                let mut state = BTreeMap::new();
                for node in &nodes.items {
                    let Some(node_name) = &node.metadata.name else { continue };

                    let lp =
                        ListParams::default().fields(format!("spec.nodeName={node_name}").as_str());
                    let node_pods = pods.list(&lp).await?;
                    state.insert(node_name.to_owned(), node_pods);
                }

                state
            };

            let schedule_state = WorldState {
                nodes,
                state,
                unscheduled_pods: pods.list(&unscheduled_lp).await?,
            };

            if schedule_state.unscheduled_pods.items.is_empty() {
                return Err(color_eyre::eyre::eyre!(
                    "No unscheduled pods found after debouncing"
                ));
            }

            let target_state = match match cli.algorithm {
                crate::Algorithm::BinPacking => {
                    algorithms::bin_packing::schedule(schedule_state).await
                }
            } {
                Ok(target_state) => target_state,
                Err(err) => {
                    return Err(color_eyre::eyre::eyre!(
                        "Failed to obtain target_state: {:#?}",
                        err
                    ));
                }
            };

            // TODO: Implement a reconciler

            Ok(())
        });
    }

    Ok(())
}
