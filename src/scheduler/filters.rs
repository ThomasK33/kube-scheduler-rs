use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{Node, Pod};
use kube_quantity::{ParseQuantityError, ParsedQuantity};

pub(crate) fn is_pod_allocatable(node: &Node, pod: &Pod) -> bool {
    let Some(status) = &node.status else { return false };
    let Some(allocatable) = &status.allocatable else { return false };

    // TODO: Extract network bandwidth information from annotations
    // if let Some(annotations) = &node.metadata.annotations {
    // };

    // If there is no pod.spec one cannot make any allocation related decisions
    let Some(pod_spec) = &pod.spec else { return false };

    // Parse allocatable quantities
    let allocatable: BTreeMap<String, ParsedQuantity> = allocatable
        .iter()
        .filter_map(|(k, v)| {
			let Ok(v): Result<ParsedQuantity, ParseQuantityError> = v.try_into() else { return None };

			Some((k.clone(), v))
		})
        .collect();

    // Combine all container requests into a map of resource names to quantities
    let container_requests: BTreeMap<String, ParsedQuantity> = pod_spec
        .containers
        .iter()
        .filter_map(|container| {
            let Some(resources) = &container.resources else { return None; };
            let Some(requests) = &resources.requests else { return None; };

            Some(requests.iter()
				.filter_map(|(k, v)| {
					let Ok(v): Result<ParsedQuantity, ParseQuantityError> = v.try_into() else { return None };

					Some((k.clone(), v))
				}))
        })
        .flatten()
        .collect();

    // If all container requests quantities are smaller than the corresponding
    // allocatable quantities, then the pod is schedulable to the node
    for (resource_name, request_quantity) in container_requests {
        let Some(allocatable_quantity) = allocatable.get(&resource_name) else { return false };

        if &request_quantity > allocatable_quantity {
            return false;
        }
    }

    true
}

pub(crate) fn is_node_schedulable(node: &Node) -> bool {
    let Some(spec) = &node.spec else { return false };
    !spec.unschedulable.unwrap_or(false)
}

pub(crate) fn is_pod_taint_toleration_fulfilled(node: &Node, pod: &Pod) -> bool {
    let Some(spec) = &node.spec else { return false };
    // TODO: Implement taint filtering

    true
}

pub(crate) fn is_pod_anti_affinity_fulfilled(node: &Node, pod: &Pod) -> bool {
    // TODO: Implement anti-affinity filtering

    true
}

pub(crate) fn is_pod_affinity_fulfilled(node: &Node, pod: &Pod) -> bool {
    // TODO: Implement affinity filtering

    true
}

#[cfg(test)]
mod tests {
    use k8s_openapi::{
        api::core::v1::{Container, NodeSpec, NodeStatus, PodSpec, ResourceRequirements},
        apimachinery::pkg::api::resource::Quantity,
    };

    use super::*;

    #[test]
    fn test_schedulable_node() {
        assert!(is_node_schedulable(&Node {
            spec: Some(NodeSpec {
                ..Default::default()
            }),
            ..Default::default()
        }));
    }

    #[test]
    fn test_unschedulable_node_explicit() {
        assert!(!is_node_schedulable(&Node {
            spec: Some(NodeSpec {
                unschedulable: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        }));
    }

    #[test]
    fn test_unschedulable_node_empty() {
        assert!(!is_node_schedulable(&Node::default()));
    }

    #[test]
    fn test_pod_schedulable_to_node() {
        let node = Node {
            status: Some(NodeStatus {
                allocatable: Some(BTreeMap::from_iter(vec![
                    ("cpu".to_string(), Quantity("2".to_string())),
                    ("memory".to_string(), Quantity("2Gi".to_string())),
                ])),
                ..Default::default()
            }),
            ..Default::default()
        };

        let pod = Pod {
            spec: Some(PodSpec {
                containers: vec![Container {
                    resources: Some(ResourceRequirements {
                        requests: Some(BTreeMap::from_iter(vec![
                            ("cpu".to_string(), Quantity("1".to_string())),
                            ("memory".to_string(), Quantity("1Gi".to_string())),
                        ])),
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };

        assert!(is_pod_allocatable(&node, &pod));
    }

    #[test]
    fn test_pod_not_schedulable_to_node() {
        let node = Node {
            status: Some(NodeStatus {
                allocatable: Some(BTreeMap::from_iter(vec![
                    ("cpu".to_string(), Quantity("2".to_string())),
                    ("memory".to_string(), Quantity("2Gi".to_string())),
                ])),
                ..Default::default()
            }),
            ..Default::default()
        };

        let pod = Pod {
            spec: Some(PodSpec {
                containers: vec![Container {
                    resources: Some(ResourceRequirements {
                        requests: Some(BTreeMap::from_iter(vec![
                            ("cpu".to_string(), Quantity("3".to_string())),
                            ("memory".to_string(), Quantity("3Gi".to_string())),
                        ])),
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };

        assert!(!is_pod_allocatable(&node, &pod));
    }

    #[test]
    fn test_pod_not_schedulable_to_node_missing_allocatable() {
        let node = Node::default();

        let pod = Pod {
            spec: Some(PodSpec {
                containers: vec![Container {
                    resources: Some(ResourceRequirements {
                        requests: Some(BTreeMap::from_iter(vec![
                            ("cpu".to_string(), Quantity("3".to_string())),
                            ("memory".to_string(), Quantity("3Gi".to_string())),
                        ])),
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };

        assert!(!is_pod_allocatable(&node, &pod));
    }

    #[test]
    fn test_pod_not_schedulable_to_node_missing_allocatable_2() {
        let node = Node {
            status: Some(NodeStatus {
                allocatable: Some(BTreeMap::from_iter(vec![
                    ("cpu".to_string(), Quantity("2".to_string())),
                    ("memory".to_string(), Quantity("2Gi".to_string())),
                ])),
                ..Default::default()
            }),
            ..Default::default()
        };

        let pod = Pod {
            spec: Some(PodSpec {
                containers: vec![Container {
                    resources: Some(ResourceRequirements {
                        requests: Some(BTreeMap::from_iter(vec![
                            ("cpu".to_string(), Quantity("3".to_string())),
                            ("memory".to_string(), Quantity("3Gi".to_string())),
                        ])),
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };

        assert!(!is_pod_allocatable(&node, &pod));
    }
}
