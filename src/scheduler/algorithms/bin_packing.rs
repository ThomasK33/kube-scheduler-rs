use color_eyre::Result;
use k8s_openapi::{
    api::core::v1::{Binding, Pod},
    apimachinery::pkg::apis::meta::v1::Status,
};
use kube::{api::PostParams, Api};

use crate::scheduler::SchedulingParameters;

pub(crate) async fn schedule(params: SchedulingParameters) -> Result<()> {
    let SchedulingParameters {
        all_nodes,
        // all_pods,
        unscheduled_pods,
        scheduler_name,
        client,
    } = params;

    // This is just a dummy scheduler assigning to the first node in the nodes
    // vector

    if let Some(first_node) = all_nodes.into_iter().next() {
        if let Some(node_name) = first_node.metadata.name {
            for pod in unscheduled_pods {
                let Some(name) = pod.metadata.name else { continue };
                let Some(namespace) = pod.metadata.namespace else { continue };

                let pods: Api<Pod> = Api::namespaced(client.clone(), &namespace);

                let res: Result<Status, kube::Error> = pods
                    .create_subresource(
                        "binding",
                        &name.clone(),
                        &PostParams {
                            field_manager: Some(scheduler_name.clone()),
                            ..Default::default()
                        },
                        serde_json::to_vec(&Binding {
                            metadata: kube::core::ObjectMeta {
                                name: Some(name.clone()),
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
                let Some(code) =  status.code else {color_eyre::eyre::bail!("Could not obtain status code from kubernetes response")};

                if code >= 200 && code <= 202 {
                    tracing::info!("Assigned pod {name} to node {node_name}");
                } else {
                    tracing::error!(
                        "An error occurred while trying to bind pod to node: {status:?}"
                    );
                }
            }
        }
    }

    Ok(())
}
