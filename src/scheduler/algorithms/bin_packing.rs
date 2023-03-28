use std::collections::BTreeMap;

use color_eyre::Result;
use k8s_openapi::api::core::v1::{Node, Pod};
use kube_quantity::ParsedQuantity;

use crate::scheduler::{
    filters::{
        is_node_schedulable, is_pod_affinity_fulfilled, is_pod_allocatable,
        is_pod_anti_affinity_fulfilled, is_pod_taint_toleration_fulfilled,
    },
    Reason, TargetState, WorldState,
};

pub(crate) async fn schedule(params: WorldState) -> Result<TargetState> {
    let WorldState {
        nodes,
        unscheduled_pods,
        state,
    } = params;

    let mut state: BTreeMap<String, Vec<Pod>> =
        state.into_iter().map(|(k, v)| (k, v.items)).collect();

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

    let mut newly_unscheduled_pods: Vec<(Pod, Reason)> = vec![];

    for pod in unscheduled_pods {
        // Filter out unfeasible nodes
        let feasible_nodes: Vec<&Node> = nodes
            .iter()
            // Filter schedulable nodes
            .filter(|node| is_node_schedulable(node))
            // Filter nodes that have enough allocatable resources for pod
            .filter(|node| is_pod_allocatable(node, &pod))
            // Filter nodes fulfilling taint toleration
            .filter(|node| is_pod_taint_toleration_fulfilled(node, &pod))
            // Filter nodes fulfilling affinities
            .filter(|node| is_pod_affinity_fulfilled(node, &pod))
            // Filter nodes fulfilling anti-affinities
            .filter(|node| is_pod_anti_affinity_fulfilled(node, &pod))
            .collect();

        if feasible_nodes.is_empty() {
            newly_unscheduled_pods.push((pod, Reason::NoFeasibleNode));
            continue;
        }

        // Score nodes

        // Score each feasible node based on the following criteria:
        // - Number of pods on node
        // - Resource availabilities on node
        struct ScoreCriteria {
            number_of_pods: u64,
            resource_availabilities: BTreeMap<String, ParsedQuantity>,
        }

        let node_scores: Vec<(&Node, ScoreCriteria)> = feasible_nodes
            .into_iter()
            .filter_map(|node| {
                let Some(node_name) = &node.metadata.name else { return None; };
                let number_of_pods = state.get(node_name)?.len().try_into().ok()?;

                let Some(node_status) = &node.status else { return None; };
                let Some(allocatable) = &node_status.allocatable else { return None };

                let resource_availabilities = allocatable
                    .iter()
                    .filter_map(|(k, v)| {
                        let v = v.try_into().ok()?;

                        Some((k.clone(), v))
                    })
                    .collect();

                Some((
                    node,
                    ScoreCriteria {
                        number_of_pods,
                        resource_availabilities,
                    },
                ))
            })
            .collect();

        // TODO: Normalize scores

        // Normalize the scores of each node based on the following criteria:
        // - Number of pods on node normalized by maximum amount of pods on node
        // - Resource availabilities on node normalized by maximum resource availabilities on node

        // Update state of scheduled pods to nodes
        let Some((node, _)) = node_scores.get(0) else {
			newly_unscheduled_pods.push((pod, Reason::NoFeasibleNode));
			continue;
		};
        let Some(node_name) = &node.metadata.name else {
			newly_unscheduled_pods.push((pod, Reason::NodeName));
			continue;
		};
        let Some(node_pods) = state.get_mut(node_name) else {
			newly_unscheduled_pods.push((pod, Reason::NodePods));
			// TODO: Potentially add a verbose error message
			continue;
		};
        node_pods.push(pod);
    }

    Ok(TargetState {
        unscheduled_pods: newly_unscheduled_pods,
        state,
    })
}
