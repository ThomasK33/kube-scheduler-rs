use color_eyre::Result;
use k8s_openapi::api::core::v1::{Node, Pod};
use kube::{api::ListParams, Api};

use crate::scheduler::{
    bind_pod_to_node,
    filters::{is_node_schedulable, is_pod_allocatable, is_pod_taint_toleration_fulfilled},
    PodBindParameters, SchedulingParameters,
};

pub(crate) async fn schedule(params: SchedulingParameters) -> Result<()> {
    let SchedulingParameters {
        unscheduled_pods,
        scheduler_name,
        client,
    } = params;

    // FIXME: This is just a dummy scheduler assigning to the first node in the nodes
    // vector

    // TODO: Sort unscheduled pods
    // Potentially make use of priority classes here
    // as of now resort to a pod's QoS
    // https://kubernetes.io/docs/concepts/scheduling-eviction/node-pressure-eviction/#node-out-of-memory-behavior

    let unscheduled_pods: Vec<Pod> = {
        let mut unscheduled_pods: Vec<(i32, Pod)> = unscheduled_pods
            .into_iter()
            .map(|pod| {
                if let Some(status) = &pod.status {
                    match &status.qos_class {
                        Some(qos_class) if qos_class == "Guaranteed" => (-997, pod),
                        Some(qos_class) if qos_class == "BestEffort" => (1000, pod),
                        // TODO: Calculate a score for burstable workloads
                        Some(qos_class) if qos_class == "Burstable" => (0, pod),
                        Some(_) => (1000, pod),
                        None => (1000, pod),
                    }
                } else {
                    // Handle the same as BestEffort pods
                    (1000, pod)
                }
            })
            .collect();
        unscheduled_pods.sort_by(|a, b| a.0.cmp(&b.0));

        unscheduled_pods.into_iter().map(|(_, pod)| pod).collect()
    };

    let nodes: Api<Node> = Api::all(client.clone());

    for pod in unscheduled_pods {
        // TODO: Filter out unfeasible nodes
        let feasible_nodes: Vec<Node> = nodes
            .list(&ListParams::default())
            .await?
            .into_iter()
            // Filter schedulable nodes
            .filter(is_node_schedulable)
            // Filter nodes that have enough allocatable resources for pod
            .filter(|node| is_pod_allocatable(node, &pod))
            // Filter nodes fulfilling taint toleration
            .filter(|node| is_pod_taint_toleration_fulfilled(node, &pod))
            // TODO: Filter nodes fulfilling affinities
            // TODO: Filter nodes fulfilling anti-affinities
            .collect();
        // TODO: Score nodes
        // TODO: Normalize scores
        // TODO: Update custom state/cache of scheduled pods to nodes
        // TODO: Bind pod to node

        let Some(node) = feasible_nodes.get(0) else { continue; };
        let Some(node_name) = &node.metadata.name else { continue; };

        let Some(name) = pod.metadata.name else { continue };
        let Some(namespace) = pod.metadata.namespace else { continue };

        let res = bind_pod_to_node(PodBindParameters {
            client: client.clone(),
            pod_name: name.clone(),
            pod_namespace: namespace.clone(),
            node_name: node_name.clone(),
            scheduler_name: scheduler_name.clone(),
        })
        .await;

        match res {
            Ok(_) => tracing::info!("Assigned pod {namespace}/{name} to node {node_name}"),
            Err(err) => {
                tracing::error!("An error occurred while trying to bind pod to node: {err:?}")
            }
        };
    }

    Ok(())
}
