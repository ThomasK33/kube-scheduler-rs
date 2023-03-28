use color_eyre::Result;
use k8s_openapi::{
    api::core::v1::{Binding, Pod},
    apimachinery::pkg::apis::meta::v1::Status,
};
use kube::{api::PostParams, Api, Client};

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

    if (200..=202).contains(&code) {
        Ok(())
    } else {
        Err(color_eyre::eyre::eyre!(
            "An error occurred while trying to bind pod to node: {status:?}"
        ))
    }
}
