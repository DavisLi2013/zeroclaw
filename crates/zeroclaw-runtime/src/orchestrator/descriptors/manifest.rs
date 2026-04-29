use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DescriptorManifest {
    pub descriptor_id: String,
    pub name: String,
    pub version: String,
    pub publisher: Option<String>,
    pub visibility: String,
    pub install_scope: String,
    pub dependency_refs: Vec<String>,
    pub permission_budget: Vec<String>,
    pub review_status: String,
    pub signature: Option<String>,
    pub compatibility: Vec<String>,
    pub status: String,
}

impl DescriptorManifest {
    pub fn new(descriptor_id: impl Into<String>, version: impl Into<String>) -> Self {
        let descriptor_id = descriptor_id.into();
        Self {
            name: descriptor_id.clone(),
            descriptor_id,
            version: version.into(),
            publisher: None,
            visibility: "private".to_string(),
            install_scope: "user".to_string(),
            dependency_refs: Vec::new(),
            permission_budget: Vec::new(),
            review_status: "draft".to_string(),
            signature: None,
            compatibility: Vec::new(),
            status: "inactive".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_manifest_new_sets_core_identity_fields() {
        let manifest = DescriptorManifest::new("capability-a", "0.1.0");

        assert_eq!(manifest.descriptor_id, "capability-a");
        assert_eq!(manifest.version, "0.1.0");
        assert_eq!(manifest.review_status, "draft");
    }
}
