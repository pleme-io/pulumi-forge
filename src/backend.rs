use std::collections::BTreeMap;

use iac_forge::IacForgeError;
use iac_forge::backend::{ArtifactKind, Backend, GeneratedArtifact, NamingConvention};
use iac_forge::ir::{IacAttribute, IacDataSource, IacProvider, IacResource, IacType};
use iac_forge::naming::{strip_provider_prefix, to_camel_case};

use crate::schema::{
    FunctionSchema, ObjectTypeSpec, PropertySpec, ProviderResource, PulumiSchema, ResourceSchema,
};

/// Pulumi backend that generates `schema.json` from the IaC forge IR.
pub struct PulumiBackend {
    naming: PulumiNaming,
}

struct PulumiNaming;

impl PulumiBackend {
    #[must_use]
    pub fn new() -> Self {
        Self {
            naming: PulumiNaming,
        }
    }

    /// Generate a complete Pulumi schema.json from provider, resources, and data sources.
    ///
    /// # Errors
    ///
    /// Returns an error if schema generation fails.
    pub fn generate_schema(
        &self,
        provider: &IacProvider,
        resources: &[IacResource],
        data_sources: &[IacDataSource],
    ) -> Result<PulumiSchema, IacForgeError> {
        let module = provider
            .platform_config
            .get("pulumi")
            .and_then(|v| v.get("module"))
            .and_then(|v| v.as_str())
            .unwrap_or("index");

        let mut schema_resources = BTreeMap::new();
        for res in resources {
            let short = strip_provider_prefix(&res.name, &provider.name);
            let type_token = format!(
                "{}:{}:{}",
                provider.name,
                module,
                to_pascal_case_custom(short)
            );
            schema_resources.insert(type_token, self.resource_to_schema(res));
        }

        let mut functions = BTreeMap::new();
        for ds in data_sources {
            let short = strip_provider_prefix(&ds.name, &provider.name);
            let type_token = format!(
                "{}:{}:get{}",
                provider.name,
                module,
                to_pascal_case_custom(short)
            );
            functions.insert(type_token, self.data_source_to_function(ds));
        }

        let provider_props = self.provider_input_properties(provider);

        // Build config section from provider auth fields
        let mut config = BTreeMap::new();
        if !provider_props.is_empty() {
            config.insert(
                "variables".to_string(),
                serde_json::to_value(&provider_props).unwrap_or_else(|_| serde_json::json!({})),
            );
        }

        Ok(PulumiSchema {
            name: provider.name.clone(),
            display_name: Some(capitalize_first(&provider.name)),
            version: provider.version.clone(),
            description: Some(provider.description.clone()),
            homepage: None,
            repository: None,
            publisher: None,
            config,
            provider: ProviderResource {
                description: Some(provider.description.clone()),
                input_properties: provider_props,
                required_inputs: vec![],
            },
            resources: schema_resources,
            functions,
            types: BTreeMap::new(),
            language: serde_json::json!({
                "nodejs": {
                    "packageName": format!("@pulumi/{}", provider.name),
                },
                "python": {
                    "packageName": format!("pulumi_{}", provider.name),
                },
                "go": {
                    "generateResourceContainerTypes": true,
                },
            }),
        })
    }

    fn provider_input_properties(&self, provider: &IacProvider) -> BTreeMap<String, PropertySpec> {
        let mut props = BTreeMap::new();
        if !provider.auth.gateway_url_field.is_empty() {
            props.insert(
                to_camel_case(&provider.auth.gateway_url_field),
                PropertySpec {
                    schema_type: Some("string".to_string()),
                    description: Some("API gateway URL".to_string()),
                    secret: None,
                    default: None,
                    items: None,
                    additional_properties: None,
                    ref_path: None,
                    replace_on_changes: None,
                    enum_values: None,
                },
            );
        }
        if !provider.auth.token_field.is_empty() {
            props.insert(
                to_camel_case(&provider.auth.token_field),
                PropertySpec {
                    schema_type: Some("string".to_string()),
                    description: Some("Access token".to_string()),
                    secret: Some(true),
                    default: None,
                    items: None,
                    additional_properties: None,
                    ref_path: None,
                    replace_on_changes: None,
                    enum_values: None,
                },
            );
        }
        props
    }

    fn resource_to_schema(&self, resource: &IacResource) -> ResourceSchema {
        let mut input_properties = BTreeMap::new();
        let mut properties = BTreeMap::new();
        let mut required_inputs = Vec::new();
        let mut required_outputs = Vec::new();

        for attr in &resource.attributes {
            let name = to_camel_case(&attr.canonical_name);
            let prop = iac_attr_to_property(attr);

            if !attr.computed || (attr.computed && attr.required) {
                input_properties.insert(name.clone(), prop.clone());
                if attr.required {
                    required_inputs.push(name.clone());
                }
            }

            properties.insert(name.clone(), prop);
            if attr.required || attr.computed {
                required_outputs.push(name);
            }
        }

        ResourceSchema {
            description: if resource.description.is_empty() {
                None
            } else {
                Some(resource.description.clone())
            },
            input_properties,
            required_inputs,
            properties,
            required: required_outputs,
        }
    }

    fn data_source_to_function(&self, ds: &IacDataSource) -> FunctionSchema {
        let mut input_props = BTreeMap::new();
        let mut output_props = BTreeMap::new();
        let mut input_required = Vec::new();
        let mut output_required = Vec::new();

        for attr in &ds.attributes {
            let name = to_camel_case(&attr.canonical_name);
            let prop = iac_attr_to_property(attr);

            if !attr.computed {
                input_props.insert(name.clone(), prop.clone());
                if attr.required {
                    input_required.push(name.clone());
                }
            }

            output_props.insert(name.clone(), prop);
            if attr.computed {
                output_required.push(name);
            }
        }

        FunctionSchema {
            description: if ds.description.is_empty() {
                None
            } else {
                Some(ds.description.clone())
            },
            inputs: Some(ObjectTypeSpec {
                properties: input_props,
                required: input_required,
            }),
            outputs: Some(ObjectTypeSpec {
                properties: output_props,
                required: output_required,
            }),
        }
    }
}

impl Default for PulumiBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl NamingConvention for PulumiNaming {
    fn resource_type_name(&self, resource_name: &str, provider_name: &str) -> String {
        let short = strip_provider_prefix(resource_name, provider_name);
        to_pascal_case_custom(short)
    }

    fn file_name(&self, _resource_name: &str, _kind: &ArtifactKind) -> String {
        "schema.json".to_string()
    }

    fn field_name(&self, api_name: &str) -> String {
        to_camel_case(api_name)
    }
}

impl Backend for PulumiBackend {
    fn platform(&self) -> &str {
        "pulumi"
    }

    fn generate_resource(
        &self,
        _resource: &IacResource,
        _provider: &IacProvider,
    ) -> Result<Vec<GeneratedArtifact>, IacForgeError> {
        // Pulumi generates a single schema.json for all resources at once.
        // Individual resource generation is a no-op; use generate_provider instead.
        Ok(vec![])
    }

    fn generate_data_source(
        &self,
        _ds: &IacDataSource,
        _provider: &IacProvider,
    ) -> Result<Vec<GeneratedArtifact>, IacForgeError> {
        Ok(vec![])
    }

    fn generate_provider(
        &self,
        provider: &IacProvider,
        resources: &[IacResource],
        data_sources: &[IacDataSource],
    ) -> Result<Vec<GeneratedArtifact>, IacForgeError> {
        let schema = self.generate_schema(provider, resources, data_sources)?;
        let json = serde_json::to_string_pretty(&schema)
            .map_err(|e| IacForgeError::BackendError(e.to_string()))?;
        Ok(vec![GeneratedArtifact {
            path: "schema.json".to_string(),
            content: json,
            kind: ArtifactKind::Schema,
        }])
    }

    fn generate_test(
        &self,
        _resource: &IacResource,
        _provider: &IacProvider,
    ) -> Result<Vec<GeneratedArtifact>, IacForgeError> {
        Ok(vec![])
    }

    fn naming(&self) -> &dyn NamingConvention {
        &self.naming
    }
}

/// Convert an `IacAttribute` to a Pulumi `PropertySpec`.
fn iac_attr_to_property(attr: &IacAttribute) -> PropertySpec {
    let (schema_type, items, additional_properties, enum_values) =
        iac_type_to_pulumi(&attr.iac_type);

    PropertySpec {
        schema_type,
        description: if attr.description.is_empty() {
            None
        } else {
            Some(attr.description.clone())
        },
        secret: if attr.sensitive { Some(true) } else { None },
        default: attr.default_value.clone(),
        items,
        additional_properties,
        ref_path: None,
        replace_on_changes: if attr.immutable { Some(true) } else { None },
        enum_values,
    }
}

/// Build a complete `PropertySpec` from an `IacType`, preserving nested structure.
fn iac_type_to_property_spec(iac_type: &IacType) -> PropertySpec {
    let (schema_type, items, additional_properties, enum_values) = iac_type_to_pulumi(iac_type);
    PropertySpec {
        schema_type,
        description: None,
        secret: None,
        default: None,
        items,
        additional_properties,
        ref_path: None,
        replace_on_changes: None,
        enum_values,
    }
}

/// Map `IacType` to Pulumi schema type components.
fn iac_type_to_pulumi(
    iac_type: &IacType,
) -> (
    Option<String>,
    Option<Box<PropertySpec>>,
    Option<Box<PropertySpec>>,
    Option<Vec<serde_json::Value>>,
) {
    match iac_type {
        IacType::String => (Some("string".into()), None, None, None),
        IacType::Integer => (Some("integer".into()), None, None, None),
        IacType::Float => (Some("number".into()), None, None, None),
        IacType::Boolean => (Some("boolean".into()), None, None, None),
        IacType::List(inner) | IacType::Set(inner) => {
            let inner_prop = iac_type_to_property_spec(inner);
            (Some("array".into()), Some(Box::new(inner_prop)), None, None)
        }
        IacType::Map(inner) => {
            let inner_prop = iac_type_to_property_spec(inner);
            (
                Some("object".into()),
                None,
                Some(Box::new(inner_prop)),
                None,
            )
        }
        // NOTE: IacType::Object fields are always empty per iac-forge design.
        // Full $ref support (types section population) is a known limitation.
        IacType::Object { .. } => (Some("object".into()), None, None, None),
        IacType::Enum { values, underlying } => {
            let (base_type, _, _, _) = iac_type_to_pulumi(underlying);
            let vals: Vec<serde_json::Value> = values
                .iter()
                .map(|v| match underlying.as_ref() {
                    IacType::Integer => v.parse::<i64>().map_or_else(
                        |_| serde_json::Value::String(v.clone()),
                        |n| serde_json::json!(n),
                    ),
                    IacType::Float => v.parse::<f64>().map_or_else(
                        |_| serde_json::Value::String(v.clone()),
                        |n| serde_json::json!(n),
                    ),
                    IacType::Boolean => match v.as_str() {
                        "true" => serde_json::Value::Bool(true),
                        "false" => serde_json::Value::Bool(false),
                        _ => serde_json::Value::String(v.clone()),
                    },
                    _ => serde_json::Value::String(v.clone()),
                })
                .collect();
            (base_type, None, None, Some(vals))
        }
        IacType::Any => (Some("string".into()), None, None, None),
    }
}

/// Simple `PascalCase` converter (hyphens and underscores are separators).
fn to_pascal_case_custom(name: &str) -> String {
    iac_forge::to_pascal_case(name)
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => {
            let upper: String = c.to_uppercase().collect();
            format!("{upper}{}", chars.as_str())
        }
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use iac_forge::ir::{AuthInfo, CrudInfo, IdentityInfo};

    use super::*;

    fn test_provider() -> IacProvider {
        IacProvider {
            name: "acme".to_string(),
            description: "Acme cloud provider".to_string(),
            version: "1.0.0".to_string(),
            auth: AuthInfo {
                token_field: "api_token".to_string(),
                env_var: "ACME_TOKEN".to_string(),
                gateway_url_field: "api_url".to_string(),
                gateway_env_var: "ACME_URL".to_string(),
            },
            skip_fields: vec![],
            platform_config: HashMap::new(),
        }
    }

    fn test_crud() -> CrudInfo {
        CrudInfo {
            create_endpoint: "POST /resources".to_string(),
            create_schema: "CreateRequest".to_string(),
            update_endpoint: Some("PUT /resources/{id}".to_string()),
            update_schema: Some("UpdateRequest".to_string()),
            read_endpoint: "GET /resources/{id}".to_string(),
            read_schema: "ReadRequest".to_string(),
            read_response_schema: None,
            delete_endpoint: "DELETE /resources/{id}".to_string(),
            delete_schema: "DeleteRequest".to_string(),
        }
    }

    fn test_identity() -> IdentityInfo {
        IdentityInfo {
            id_field: "id".to_string(),
            import_field: "name".to_string(),
            force_replace_fields: vec![],
        }
    }

    fn test_resource() -> IacResource {
        IacResource {
            name: "acme_static_secret".to_string(),
            description: "A static secret resource".to_string(),
            category: "secrets".to_string(),
            crud: test_crud(),
            attributes: vec![
                IacAttribute {
                    api_name: "name".to_string(),
                    canonical_name: "name".to_string(),
                    description: "The secret name".to_string(),
                    iac_type: IacType::String,
                    required: true,
                    computed: false,
                    sensitive: false,
                    immutable: true,
                    default_value: None,
                    enum_values: None,
                    read_path: None,
                    update_only: false,
                },
                IacAttribute {
                    api_name: "value".to_string(),
                    canonical_name: "value".to_string(),
                    description: "The secret value".to_string(),
                    iac_type: IacType::String,
                    required: false,
                    computed: false,
                    sensitive: true,
                    immutable: false,
                    default_value: None,
                    enum_values: None,
                    read_path: None,
                    update_only: false,
                },
                IacAttribute {
                    api_name: "tags".to_string(),
                    canonical_name: "tags".to_string(),
                    description: "Tags for the secret".to_string(),
                    iac_type: IacType::List(Box::new(IacType::String)),
                    required: false,
                    computed: false,
                    sensitive: false,
                    immutable: false,
                    default_value: None,
                    enum_values: None,
                    read_path: None,
                    update_only: false,
                },
                IacAttribute {
                    api_name: "metadata".to_string(),
                    canonical_name: "metadata".to_string(),
                    description: "Metadata map".to_string(),
                    iac_type: IacType::Map(Box::new(IacType::String)),
                    required: false,
                    computed: false,
                    sensitive: false,
                    immutable: false,
                    default_value: None,
                    enum_values: None,
                    read_path: None,
                    update_only: false,
                },
                IacAttribute {
                    api_name: "enabled".to_string(),
                    canonical_name: "enabled".to_string(),
                    description: "Whether the secret is enabled".to_string(),
                    iac_type: IacType::Boolean,
                    required: false,
                    computed: false,
                    sensitive: false,
                    immutable: false,
                    default_value: Some(serde_json::Value::Bool(true)),
                    enum_values: None,
                    read_path: None,
                    update_only: false,
                },
                IacAttribute {
                    api_name: "id".to_string(),
                    canonical_name: "id".to_string(),
                    description: "The resource ID".to_string(),
                    iac_type: IacType::String,
                    required: false,
                    computed: true,
                    sensitive: false,
                    immutable: false,
                    default_value: None,
                    enum_values: None,
                    read_path: None,
                    update_only: false,
                },
            ],
            identity: test_identity(),
        }
    }

    fn test_data_source() -> IacDataSource {
        IacDataSource {
            name: "acme_secret_value".to_string(),
            description: "Read a secret value".to_string(),
            read_endpoint: "GET /secrets/{name}".to_string(),
            read_schema: "GetSecretRequest".to_string(),
            read_response_schema: Some("GetSecretResponse".to_string()),
            attributes: vec![
                IacAttribute {
                    api_name: "name".to_string(),
                    canonical_name: "name".to_string(),
                    description: "Secret name to look up".to_string(),
                    iac_type: IacType::String,
                    required: true,
                    computed: false,
                    sensitive: false,
                    immutable: false,
                    default_value: None,
                    enum_values: None,
                    read_path: None,
                    update_only: false,
                },
                IacAttribute {
                    api_name: "value".to_string(),
                    canonical_name: "value".to_string(),
                    description: "The secret value".to_string(),
                    iac_type: IacType::String,
                    required: false,
                    computed: true,
                    sensitive: true,
                    immutable: false,
                    default_value: None,
                    enum_values: None,
                    read_path: None,
                    update_only: false,
                },
            ],
        }
    }

    #[test]
    fn schema_has_correct_provider_name() {
        let backend = PulumiBackend::new();
        let provider = test_provider();
        let schema = backend
            .generate_schema(&provider, &[], &[])
            .expect("schema generation should succeed");

        assert_eq!(schema.name, "acme");
        assert_eq!(schema.version, "1.0.0");
        assert_eq!(schema.display_name.as_deref(), Some("Acme"));
        assert_eq!(schema.description.as_deref(), Some("Acme cloud provider"));
    }

    #[test]
    fn resources_have_correct_type_tokens() {
        let backend = PulumiBackend::new();
        let provider = test_provider();
        let resource = test_resource();
        let schema = backend
            .generate_schema(&provider, &[resource], &[])
            .expect("schema generation should succeed");

        assert!(
            schema.resources.contains_key("acme:index:StaticSecret"),
            "expected type token acme:index:StaticSecret, got keys: {:?}",
            schema.resources.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn properties_map_correctly() {
        let backend = PulumiBackend::new();
        let provider = test_provider();
        let resource = test_resource();
        let schema = backend
            .generate_schema(&provider, &[resource], &[])
            .expect("schema generation should succeed");

        let res = &schema.resources["acme:index:StaticSecret"];

        // String property
        let name_prop = &res.properties["name"];
        assert_eq!(name_prop.schema_type.as_deref(), Some("string"));

        // Boolean property
        let enabled_prop = &res.properties["enabled"];
        assert_eq!(enabled_prop.schema_type.as_deref(), Some("boolean"));
        assert_eq!(enabled_prop.default, Some(serde_json::Value::Bool(true)));

        // List property
        let tags_prop = &res.properties["tags"];
        assert_eq!(tags_prop.schema_type.as_deref(), Some("array"));
        assert!(tags_prop.items.is_some());
        let items = tags_prop.items.as_ref().unwrap();
        assert_eq!(items.schema_type.as_deref(), Some("string"));

        // Map property
        let meta_prop = &res.properties["metadata"];
        assert_eq!(meta_prop.schema_type.as_deref(), Some("object"));
        assert!(meta_prop.additional_properties.is_some());
        let addl = meta_prop.additional_properties.as_ref().unwrap();
        assert_eq!(addl.schema_type.as_deref(), Some("string"));
    }

    #[test]
    fn sensitive_fields_get_secret_true() {
        let backend = PulumiBackend::new();
        let provider = test_provider();
        let resource = test_resource();
        let schema = backend
            .generate_schema(&provider, &[resource], &[])
            .expect("schema generation should succeed");

        let res = &schema.resources["acme:index:StaticSecret"];

        let value_prop = &res.properties["value"];
        assert_eq!(value_prop.secret, Some(true));

        // Non-sensitive field should not have secret
        let name_prop = &res.properties["name"];
        assert_eq!(name_prop.secret, None);
    }

    #[test]
    fn immutable_fields_get_replace_on_changes() {
        let backend = PulumiBackend::new();
        let provider = test_provider();
        let resource = test_resource();
        let schema = backend
            .generate_schema(&provider, &[resource], &[])
            .expect("schema generation should succeed");

        let res = &schema.resources["acme:index:StaticSecret"];

        let name_prop = &res.properties["name"];
        assert_eq!(name_prop.replace_on_changes, Some(true));

        // Non-immutable field should not have replaceOnChanges
        let value_prop = &res.properties["value"];
        assert_eq!(value_prop.replace_on_changes, None);
    }

    #[test]
    fn required_fields_in_required_inputs() {
        let backend = PulumiBackend::new();
        let provider = test_provider();
        let resource = test_resource();
        let schema = backend
            .generate_schema(&provider, &[resource], &[])
            .expect("schema generation should succeed");

        let res = &schema.resources["acme:index:StaticSecret"];

        assert!(
            res.required_inputs.contains(&"name".to_string()),
            "required_inputs should contain 'name'"
        );
        assert!(
            !res.required_inputs.contains(&"value".to_string()),
            "required_inputs should not contain 'value' (not required)"
        );
    }

    #[test]
    fn computed_fields_excluded_from_inputs() {
        let backend = PulumiBackend::new();
        let provider = test_provider();
        let resource = test_resource();
        let schema = backend
            .generate_schema(&provider, &[resource], &[])
            .expect("schema generation should succeed");

        let res = &schema.resources["acme:index:StaticSecret"];

        // Computed-only field 'id' should not be an input
        assert!(
            !res.input_properties.contains_key("id"),
            "computed-only field 'id' should not appear in input_properties"
        );
        // But should appear in output properties
        assert!(
            res.properties.contains_key("id"),
            "computed field 'id' should appear in properties"
        );
    }

    #[test]
    fn data_sources_produce_functions() {
        let backend = PulumiBackend::new();
        let provider = test_provider();
        let ds = test_data_source();
        let schema = backend
            .generate_schema(&provider, &[], &[ds])
            .expect("schema generation should succeed");

        assert!(
            schema.functions.contains_key("acme:index:getSecretValue"),
            "expected function token acme:index:getSecretValue, got keys: {:?}",
            schema.functions.keys().collect::<Vec<_>>()
        );

        let func = &schema.functions["acme:index:getSecretValue"];
        assert!(func.inputs.is_some());
        let inputs = func.inputs.as_ref().unwrap();
        assert!(inputs.properties.contains_key("name"));
        assert!(inputs.required.contains(&"name".to_string()));

        assert!(func.outputs.is_some());
        let outputs = func.outputs.as_ref().unwrap();
        assert!(outputs.properties.contains_key("value"));
        let value_prop = &outputs.properties["value"];
        assert_eq!(value_prop.secret, Some(true));
    }

    #[test]
    fn generate_provider_produces_schema_artifact() {
        let backend = PulumiBackend::new();
        let provider = test_provider();
        let resource = test_resource();
        let ds = test_data_source();

        let artifacts = backend
            .generate_provider(&provider, &[resource], &[ds])
            .expect("generate_provider should succeed");

        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].path, "schema.json");
        assert_eq!(artifacts[0].kind, ArtifactKind::Schema);

        // Verify the content is valid JSON
        let parsed: PulumiSchema =
            serde_json::from_str(&artifacts[0].content).expect("should be valid JSON");
        assert_eq!(parsed.name, "acme");
        assert!(parsed.resources.contains_key("acme:index:StaticSecret"));
        assert!(parsed.functions.contains_key("acme:index:getSecretValue"));
    }

    #[test]
    fn provider_auth_fields_in_schema() {
        let backend = PulumiBackend::new();
        let provider = test_provider();
        let schema = backend
            .generate_schema(&provider, &[], &[])
            .expect("schema generation should succeed");

        assert!(
            schema.provider.input_properties.contains_key("apiUrl"),
            "provider should have apiUrl input"
        );
        assert!(
            schema.provider.input_properties.contains_key("apiToken"),
            "provider should have apiToken input"
        );

        let token_prop = &schema.provider.input_properties["apiToken"];
        assert_eq!(token_prop.secret, Some(true));
    }

    #[test]
    fn backend_trait_platform() {
        let backend = PulumiBackend::new();
        assert_eq!(backend.platform(), "pulumi");
    }

    #[test]
    fn naming_convention() {
        let backend = PulumiBackend::new();
        let naming = backend.naming();

        assert_eq!(
            naming.resource_type_name("acme_static_secret", "acme"),
            "StaticSecret"
        );
        assert_eq!(
            naming.file_name("anything", &ArtifactKind::Schema),
            "schema.json"
        );
        assert_eq!(naming.field_name("api_gateway_url"), "apiGatewayUrl");
    }

    #[test]
    fn default_impl() {
        let _backend = PulumiBackend::default();
    }

    #[test]
    fn enum_type_mapping() {
        let backend = PulumiBackend::new();
        let provider = test_provider();

        let resource = IacResource {
            name: "acme_auth_method".to_string(),
            description: "An auth method".to_string(),
            category: "auth".to_string(),
            crud: test_crud(),
            attributes: vec![IacAttribute {
                api_name: "method_type".to_string(),
                canonical_name: "method_type".to_string(),
                description: "The auth method type".to_string(),
                iac_type: IacType::Enum {
                    values: vec!["api_key".into(), "saml".into(), "oidc".into()],
                    underlying: Box::new(IacType::String),
                },
                required: true,
                computed: false,
                sensitive: false,
                immutable: false,
                default_value: None,
                enum_values: None,
                read_path: None,
                update_only: false,
            }],
            identity: test_identity(),
        };

        let schema = backend
            .generate_schema(&provider, &[resource], &[])
            .expect("schema generation should succeed");

        let res = &schema.resources["acme:index:AuthMethod"];
        let method_prop = &res.properties["methodType"];
        assert_eq!(method_prop.schema_type.as_deref(), Some("string"));
        assert!(method_prop.enum_values.is_some());
        let enums = method_prop.enum_values.as_ref().unwrap();
        assert_eq!(enums.len(), 3);
        assert_eq!(enums[0], serde_json::Value::String("api_key".into()));
    }

    #[test]
    fn nested_list_of_list_type_mapping() {
        let backend = PulumiBackend::new();
        let provider = test_provider();

        let resource = IacResource {
            name: "acme_matrix".to_string(),
            description: "A matrix resource".to_string(),
            category: "data".to_string(),
            crud: test_crud(),
            attributes: vec![IacAttribute {
                api_name: "grid".to_string(),
                canonical_name: "grid".to_string(),
                description: "A list of lists of strings".to_string(),
                iac_type: IacType::List(Box::new(IacType::List(Box::new(IacType::String)))),
                required: false,
                computed: false,
                sensitive: false,
                immutable: false,
                default_value: None,
                enum_values: None,
                read_path: None,
                update_only: false,
            }],
            identity: test_identity(),
        };

        let schema = backend
            .generate_schema(&provider, &[resource], &[])
            .expect("schema generation should succeed");

        let res = &schema.resources["acme:index:Matrix"];
        let grid_prop = &res.properties["grid"];

        // Outer: array
        assert_eq!(grid_prop.schema_type.as_deref(), Some("array"));
        assert!(grid_prop.items.is_some());

        // Inner items: also array
        let inner = grid_prop.items.as_ref().unwrap();
        assert_eq!(inner.schema_type.as_deref(), Some("array"));
        assert!(inner.items.is_some(), "nested list should have items");

        // Innermost: string
        let innermost = inner.items.as_ref().unwrap();
        assert_eq!(innermost.schema_type.as_deref(), Some("string"));
    }

    #[test]
    fn enum_with_integer_underlying_type() {
        let backend = PulumiBackend::new();
        let provider = test_provider();

        let resource = IacResource {
            name: "acme_priority".to_string(),
            description: "A priority resource".to_string(),
            category: "config".to_string(),
            crud: test_crud(),
            attributes: vec![IacAttribute {
                api_name: "level".to_string(),
                canonical_name: "level".to_string(),
                description: "Priority level".to_string(),
                iac_type: IacType::Enum {
                    values: vec!["1".into(), "2".into(), "3".into()],
                    underlying: Box::new(IacType::Integer),
                },
                required: true,
                computed: false,
                sensitive: false,
                immutable: false,
                default_value: None,
                enum_values: None,
                read_path: None,
                update_only: false,
            }],
            identity: test_identity(),
        };

        let schema = backend
            .generate_schema(&provider, &[resource], &[])
            .expect("schema generation should succeed");

        let res = &schema.resources["acme:index:Priority"];
        let level_prop = &res.properties["level"];
        assert_eq!(level_prop.schema_type.as_deref(), Some("integer"));
        assert!(level_prop.enum_values.is_some());
        let enums = level_prop.enum_values.as_ref().unwrap();
        assert_eq!(enums.len(), 3);
        // Values should be JSON numbers, not strings
        assert_eq!(enums[0], serde_json::json!(1));
        assert_eq!(enums[1], serde_json::json!(2));
        assert_eq!(enums[2], serde_json::json!(3));
    }

    #[test]
    fn data_source_no_inputs_produces_empty_object_type_spec() {
        let backend = PulumiBackend::new();
        let provider = test_provider();

        // Data source where all attributes are computed (no user inputs)
        let ds = IacDataSource {
            name: "acme_current_user".to_string(),
            description: "Get the current user".to_string(),
            read_endpoint: "GET /me".to_string(),
            read_schema: "GetMeRequest".to_string(),
            read_response_schema: Some("GetMeResponse".to_string()),
            attributes: vec![IacAttribute {
                api_name: "email".to_string(),
                canonical_name: "email".to_string(),
                description: "User email".to_string(),
                iac_type: IacType::String,
                required: false,
                computed: true,
                sensitive: false,
                immutable: false,
                default_value: None,
                enum_values: None,
                read_path: None,
                update_only: false,
            }],
        };

        let schema = backend
            .generate_schema(&provider, &[], &[ds])
            .expect("schema generation should succeed");

        let func = &schema.functions["acme:index:getCurrentUser"];
        // inputs should be Some with empty properties, not None
        assert!(
            func.inputs.is_some(),
            "data source with no inputs should still have Some(ObjectTypeSpec)"
        );
        let inputs = func.inputs.as_ref().unwrap();
        assert!(
            inputs.properties.is_empty(),
            "input properties should be empty"
        );
        assert!(
            inputs.required.is_empty(),
            "required inputs should be empty"
        );
    }

    #[test]
    fn provider_config_populated_from_auth() {
        let backend = PulumiBackend::new();
        let provider = test_provider();
        let schema = backend
            .generate_schema(&provider, &[], &[])
            .expect("schema generation should succeed");

        assert!(
            !schema.config.is_empty(),
            "config section should not be empty when provider has auth fields"
        );
        assert!(
            schema.config.contains_key("variables"),
            "config should have a variables key"
        );
        let variables = &schema.config["variables"];
        assert!(
            variables.get("apiUrl").is_some(),
            "config variables should contain apiUrl"
        );
        assert!(
            variables.get("apiToken").is_some(),
            "config variables should contain apiToken"
        );
    }

    #[test]
    fn language_section_populated() {
        let backend = PulumiBackend::new();
        let provider = test_provider();
        let schema = backend
            .generate_schema(&provider, &[], &[])
            .expect("schema generation should succeed");

        assert!(schema.language.get("nodejs").is_some());
        assert_eq!(schema.language["nodejs"]["packageName"], "@pulumi/acme");
        assert!(schema.language.get("python").is_some());
        assert_eq!(schema.language["python"]["packageName"], "pulumi_acme");
        assert!(schema.language.get("go").is_some());
        assert_eq!(
            schema.language["go"]["generateResourceContainerTypes"],
            true
        );
    }
}
