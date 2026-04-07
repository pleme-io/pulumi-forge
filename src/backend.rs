use std::collections::BTreeMap;

use iac_forge::IacForgeError;
use iac_forge::backend::{ArtifactKind, Backend, GeneratedArtifact, NamingConvention};
use iac_forge::ir::{IacAttribute, IacDataSource, IacProvider, IacResource, IacType};
use iac_forge::naming::{strip_provider_prefix, to_camel_case};

use crate::schema::{
    FunctionSchema, ObjectTypeSpec, PropertySpec, ProviderResource, PulumiSchema, ResourceSchema,
};

/// Decomposed components of a Pulumi property type mapping.
///
/// Fields: (`schema_type`, `items`, `additional_properties`, `enum_values`)
type TypeComponents = (
    Option<String>,
    Option<Box<PropertySpec>>,
    Option<Box<PropertySpec>>,
    Option<Vec<serde_json::Value>>,
);

/// Pulumi backend that generates `schema.json` from the `IaC` forge IR.
pub struct PulumiBackend {
    naming: PulumiNaming,
}

struct PulumiNaming;

impl PulumiBackend {
    /// Create a new Pulumi backend instance.
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
    /// Returns [`IacForgeError::BackendError`] if schema generation fails.
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
            schema_resources.insert(type_token, Self::resource_to_schema(res));
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
            functions.insert(type_token, Self::data_source_to_function(ds));
        }

        let provider_props = Self::provider_input_properties(provider);

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
                    "packageDescription": format!("A Pulumi package for managing {} resources.", capitalize_first(&provider.name)),
                    "respectSchemaVersion": true
                },
                "python": {
                    "packageName": format!("pulumi_{}", provider.name),
                    "respectSchemaVersion": true,
                    "pyproject": { "enabled": true },
                    "inputTypes": "classes-and-dicts"
                },
                "go": {
                    "importBasePath": format!("github.com/pleme-io/pulumi-{}/sdk/go/{}", provider.name, provider.name),
                    "generateResourceContainerTypes": true,
                    "respectSchemaVersion": true
                },
                "csharp": {
                    "packageReferences": { "Pulumi": "3.*" },
                    "rootNamespace": "Pulumi",
                    "respectSchemaVersion": true
                },
                "java": {
                    "basePackage": format!("com.pulumi.{}", provider.name),
                    "buildFiles": "gradle",
                    "gradleNexusPublishPluginVersion": "2.0.0",
                    "dependencies": {
                        "com.google.code.findbugs:jsr305": "3.0.2",
                        "com.google.code.gson:gson": "2.8.9",
                        "com.pulumi:pulumi": "1.0.0"
                    }
                }
            }),
        })
    }

    fn provider_input_properties(provider: &IacProvider) -> BTreeMap<String, PropertySpec> {
        let mut props = BTreeMap::new();
        if !provider.auth.gateway_url_field.is_empty() {
            let mut prop = PropertySpec::typed("string");
            prop.description = Some("API gateway URL".to_string());
            props.insert(to_camel_case(&provider.auth.gateway_url_field), prop);
        }
        if !provider.auth.token_field.is_empty() {
            let mut prop = PropertySpec::typed("string");
            prop.description = Some("Access token".to_string());
            prop.secret = Some(true);
            props.insert(to_camel_case(&provider.auth.token_field), prop);
        }
        props
    }

    fn resource_to_schema(resource: &IacResource) -> ResourceSchema {
        let mut input_properties = BTreeMap::new();
        let mut properties = BTreeMap::new();
        let mut required_inputs = Vec::new();
        let mut required_outputs = Vec::new();

        for attr in &resource.attributes {
            let name = to_camel_case(&attr.canonical_name);
            let prop = PropertySpec::from(attr);

            if !attr.computed || attr.required {
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

    fn data_source_to_function(ds: &IacDataSource) -> FunctionSchema {
        let mut input_props = BTreeMap::new();
        let mut output_props = BTreeMap::new();
        let mut input_required = Vec::new();
        let mut output_required = Vec::new();

        for attr in &ds.attributes {
            let name = to_camel_case(&attr.canonical_name);
            let prop = PropertySpec::from(attr);

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
    // TODO(iac-forge): Backend trait should return &'static str from platform()
    #[allow(clippy::unnecessary_literal_bound)]
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

impl From<&IacAttribute> for PropertySpec {
    fn from(attr: &IacAttribute) -> Self {
        let (schema_type, items, additional_properties, enum_values) =
            iac_type_to_pulumi(&attr.iac_type);

        Self {
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
}

impl From<&IacType> for PropertySpec {
    fn from(iac_type: &IacType) -> Self {
        let (schema_type, items, additional_properties, enum_values) =
            iac_type_to_pulumi(iac_type);
        Self {
            schema_type,
            items,
            additional_properties,
            enum_values,
            ..Self::default()
        }
    }
}

/// Map an `IacType` to its Pulumi schema type components.
fn iac_type_to_pulumi(iac_type: &IacType) -> TypeComponents {
    match iac_type {
        IacType::Integer => (Some("integer".into()), None, None, None),
        IacType::Float => (Some("number".into()), None, None, None),
        IacType::Boolean => (Some("boolean".into()), None, None, None),
        IacType::List(inner) | IacType::Set(inner) => {
            let inner_prop = PropertySpec::from(inner.as_ref());
            (Some("array".into()), Some(Box::new(inner_prop)), None, None)
        }
        IacType::Map(inner) => {
            let inner_prop = PropertySpec::from(inner.as_ref());
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
                .map(|v| coerce_enum_value(v, underlying))
                .collect();
            (base_type, None, None, Some(vals))
        }
        IacType::String | IacType::Any => (Some("string".into()), None, None, None),
    }
}

/// Coerce a string enum value to the appropriate JSON type based on the
/// underlying `IacType`. Falls back to a JSON string when parsing fails.
fn coerce_enum_value(v: &str, underlying: &IacType) -> serde_json::Value {
    match underlying {
        IacType::Integer => v
            .parse::<i64>()
            .map_or_else(|_| serde_json::Value::String(v.to_owned()), |n| serde_json::json!(n)),
        IacType::Float => v
            .parse::<f64>()
            .map_or_else(|_| serde_json::Value::String(v.to_owned()), |n| serde_json::json!(n)),
        IacType::Boolean => match v {
            "true" => serde_json::Value::Bool(true),
            "false" => serde_json::Value::Bool(false),
            _ => serde_json::Value::String(v.to_owned()),
        },
        _ => serde_json::Value::String(v.to_owned()),
    }
}

/// Simple `PascalCase` converter (hyphens and underscores are separators).
fn to_pascal_case_custom(name: &str) -> String {
    iac_forge::to_pascal_case(name)
}

/// Capitalize the first character of a string.
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

        // All 5 languages configured
        assert!(schema.language.get("nodejs").is_some());
        assert_eq!(schema.language["nodejs"]["packageName"], "@pulumi/acme");
        assert_eq!(schema.language["nodejs"]["respectSchemaVersion"], true);

        assert!(schema.language.get("python").is_some());
        assert_eq!(schema.language["python"]["packageName"], "pulumi_acme");
        assert_eq!(schema.language["python"]["pyproject"]["enabled"], true);
        assert_eq!(schema.language["python"]["inputTypes"], "classes-and-dicts");

        assert!(schema.language.get("go").is_some());
        assert_eq!(schema.language["go"]["generateResourceContainerTypes"], true);
        assert_eq!(schema.language["go"]["respectSchemaVersion"], true);
        assert!(schema.language["go"]["importBasePath"].as_str().unwrap().contains("pulumi-acme"));

        assert!(schema.language.get("csharp").is_some());
        assert_eq!(schema.language["csharp"]["rootNamespace"], "Pulumi");
        assert_eq!(schema.language["csharp"]["respectSchemaVersion"], true);

        assert!(schema.language.get("java").is_some());
        assert_eq!(schema.language["java"]["buildFiles"], "gradle");
        assert!(schema.language["java"]["basePackage"].as_str().unwrap().contains("acme"));
    }

    /// Build a resource with ALL IacType variants to verify exhaustive type mapping.
    fn resource_with_all_types() -> IacResource {
        IacResource {
            name: "acme_kitchen_sink".to_string(),
            description: "Resource with all type variants".to_string(),
            category: "test".to_string(),
            crud: test_crud(),
            attributes: vec![
                IacAttribute {
                    api_name: "str_field".to_string(),
                    canonical_name: "str_field".to_string(),
                    description: "A string".to_string(),
                    iac_type: IacType::String,
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
                    api_name: "int_field".to_string(),
                    canonical_name: "int_field".to_string(),
                    description: "An integer".to_string(),
                    iac_type: IacType::Integer,
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
                    api_name: "float_field".to_string(),
                    canonical_name: "float_field".to_string(),
                    description: "A float".to_string(),
                    iac_type: IacType::Float,
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
                    api_name: "bool_field".to_string(),
                    canonical_name: "bool_field".to_string(),
                    description: "A boolean".to_string(),
                    iac_type: IacType::Boolean,
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
                    api_name: "list_field".to_string(),
                    canonical_name: "list_field".to_string(),
                    description: "A list".to_string(),
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
                    api_name: "set_field".to_string(),
                    canonical_name: "set_field".to_string(),
                    description: "A set".to_string(),
                    iac_type: IacType::Set(Box::new(IacType::Integer)),
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
                    api_name: "map_field".to_string(),
                    canonical_name: "map_field".to_string(),
                    description: "A map".to_string(),
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
                    api_name: "object_field".to_string(),
                    canonical_name: "object_field".to_string(),
                    description: "An object".to_string(),
                    iac_type: IacType::Object {
                        name: "InnerObj".to_string(),
                        fields: vec![],
                    },
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
                    api_name: "enum_field".to_string(),
                    canonical_name: "enum_field".to_string(),
                    description: "An enum".to_string(),
                    iac_type: IacType::Enum {
                        values: vec!["x".into(), "y".into()],
                        underlying: Box::new(IacType::String),
                    },
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
                    api_name: "any_field".to_string(),
                    canonical_name: "any_field".to_string(),
                    description: "An any".to_string(),
                    iac_type: IacType::Any,
                    required: false,
                    computed: false,
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

    #[test]
    fn resource_with_all_iac_type_variants() {
        let backend = PulumiBackend::new();
        let provider = test_provider();
        let resource = resource_with_all_types();
        let schema = backend
            .generate_schema(&provider, &[resource], &[])
            .expect("schema generation should succeed");

        let res = &schema.resources["acme:index:KitchenSink"];

        assert_eq!(res.properties["strField"].schema_type.as_deref(), Some("string"));
        assert_eq!(res.properties["intField"].schema_type.as_deref(), Some("integer"));
        assert_eq!(res.properties["floatField"].schema_type.as_deref(), Some("number"));
        assert_eq!(res.properties["boolField"].schema_type.as_deref(), Some("boolean"));
        assert_eq!(res.properties["listField"].schema_type.as_deref(), Some("array"));
        assert!(res.properties["listField"].items.is_some());
        assert_eq!(res.properties["setField"].schema_type.as_deref(), Some("array"));
        assert!(res.properties["setField"].items.is_some());
        assert_eq!(res.properties["mapField"].schema_type.as_deref(), Some("object"));
        assert!(res.properties["mapField"].additional_properties.is_some());
        assert_eq!(res.properties["objectField"].schema_type.as_deref(), Some("object"));
        assert_eq!(res.properties["enumField"].schema_type.as_deref(), Some("string"));
        assert!(res.properties["enumField"].enum_values.is_some());
        assert_eq!(res.properties["anyField"].schema_type.as_deref(), Some("string"));
    }

    #[test]
    fn resource_with_no_attributes() {
        let backend = PulumiBackend::new();
        let provider = test_provider();

        let resource = IacResource {
            name: "acme_empty".to_string(),
            description: String::new(),
            category: "test".to_string(),
            crud: test_crud(),
            attributes: vec![],
            identity: test_identity(),
        };

        let schema = backend
            .generate_schema(&provider, &[resource], &[])
            .expect("schema generation should succeed");

        let res = &schema.resources["acme:index:Empty"];
        assert!(res.properties.is_empty());
        assert!(res.input_properties.is_empty());
        assert!(res.required_inputs.is_empty());
        assert!(res.required.is_empty());
        assert!(res.description.is_none(), "empty description should be None");
    }

    #[test]
    fn resource_with_only_computed_attributes() {
        let backend = PulumiBackend::new();
        let provider = test_provider();

        let resource = IacResource {
            name: "acme_readonly".to_string(),
            description: "Read-only resource".to_string(),
            category: "test".to_string(),
            crud: test_crud(),
            attributes: vec![
                IacAttribute {
                    api_name: "id".to_string(),
                    canonical_name: "id".to_string(),
                    description: "Computed ID".to_string(),
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
                IacAttribute {
                    api_name: "created_at".to_string(),
                    canonical_name: "created_at".to_string(),
                    description: "Creation timestamp".to_string(),
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
        };

        let schema = backend
            .generate_schema(&provider, &[resource], &[])
            .expect("schema generation should succeed");

        let res = &schema.resources["acme:index:Readonly"];
        // Computed-only fields should NOT appear in input_properties
        assert!(
            res.input_properties.is_empty(),
            "computed-only fields should not be in input_properties"
        );
        // But should appear in properties
        assert!(res.properties.contains_key("id"));
        assert!(res.properties.contains_key("createdAt"));
        // They should be required in the output (computed fields are required outputs)
        assert!(res.required.contains(&"id".to_string()));
        assert!(res.required.contains(&"createdAt".to_string()));
    }

    #[test]
    fn data_source_with_mix_of_computed_and_non_computed() {
        let backend = PulumiBackend::new();
        let provider = test_provider();

        let ds = IacDataSource {
            name: "acme_role_info".to_string(),
            description: "Get role details".to_string(),
            read_endpoint: "GET /roles/{name}".to_string(),
            read_schema: "GetRoleRequest".to_string(),
            read_response_schema: Some("GetRoleResponse".to_string()),
            attributes: vec![
                IacAttribute {
                    api_name: "name".to_string(),
                    canonical_name: "name".to_string(),
                    description: "Role name".to_string(),
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
                    api_name: "permissions".to_string(),
                    canonical_name: "permissions".to_string(),
                    description: "Permissions list".to_string(),
                    iac_type: IacType::List(Box::new(IacType::String)),
                    required: false,
                    computed: true,
                    sensitive: false,
                    immutable: false,
                    default_value: None,
                    enum_values: None,
                    read_path: None,
                    update_only: false,
                },
                IacAttribute {
                    api_name: "created_by".to_string(),
                    canonical_name: "created_by".to_string(),
                    description: "Who created it".to_string(),
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
        };

        let schema = backend
            .generate_schema(&provider, &[], &[ds])
            .expect("schema generation should succeed");

        let func = &schema.functions["acme:index:getRoleInfo"];
        let inputs = func.inputs.as_ref().unwrap();
        let outputs = func.outputs.as_ref().unwrap();

        // Only non-computed fields should be inputs
        assert!(inputs.properties.contains_key("name"));
        assert!(!inputs.properties.contains_key("permissions"));
        assert!(!inputs.properties.contains_key("createdBy"));

        // All fields should appear in outputs
        assert!(outputs.properties.contains_key("name"));
        assert!(outputs.properties.contains_key("permissions"));
        assert!(outputs.properties.contains_key("createdBy"));

        // Computed fields should be required in outputs
        assert!(outputs.required.contains(&"permissions".to_string()));
        assert!(outputs.required.contains(&"createdBy".to_string()));
    }

    #[test]
    fn provider_with_no_auth_fields() {
        let backend = PulumiBackend::new();

        let provider = IacProvider {
            name: "noauth".to_string(),
            description: "No auth provider".to_string(),
            version: "0.1.0".to_string(),
            auth: iac_forge::ir::AuthInfo {
                token_field: String::new(),
                env_var: String::new(),
                gateway_url_field: String::new(),
                gateway_env_var: String::new(),
            },
            skip_fields: vec![],
            platform_config: HashMap::new(),
        };

        let schema = backend
            .generate_schema(&provider, &[], &[])
            .expect("schema generation should succeed");

        assert!(
            schema.provider.input_properties.is_empty(),
            "provider with no auth should have empty input_properties"
        );
        assert!(
            schema.config.is_empty(),
            "config section should be empty when no auth fields"
        );
    }

    #[test]
    fn nested_list_of_map_of_string_type_mapping() {
        let backend = PulumiBackend::new();
        let provider = test_provider();

        let resource = IacResource {
            name: "acme_complex".to_string(),
            description: "Complex nested type".to_string(),
            category: "data".to_string(),
            crud: test_crud(),
            attributes: vec![IacAttribute {
                api_name: "entries".to_string(),
                canonical_name: "entries".to_string(),
                description: "List of maps of strings".to_string(),
                iac_type: IacType::List(Box::new(IacType::Map(Box::new(IacType::String)))),
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

        let res = &schema.resources["acme:index:Complex"];
        let prop = &res.properties["entries"];

        // Outer: array
        assert_eq!(prop.schema_type.as_deref(), Some("array"));
        assert!(prop.items.is_some());

        // Items: object (Map)
        let items = prop.items.as_ref().unwrap();
        assert_eq!(items.schema_type.as_deref(), Some("object"));
        assert!(items.additional_properties.is_some());

        // additional_properties: string
        let addl = items.additional_properties.as_ref().unwrap();
        assert_eq!(addl.schema_type.as_deref(), Some("string"));
    }

    #[test]
    fn schema_json_is_valid_parseable() {
        let backend = PulumiBackend::new();
        let provider = test_provider();
        let resource = test_resource();
        let ds = test_data_source();

        let artifacts = backend
            .generate_provider(&provider, &[resource], &[ds])
            .expect("generate_provider should succeed");

        let json_str = &artifacts[0].content;

        // Verify it parses as generic JSON
        let _: serde_json::Value =
            serde_json::from_str(json_str).expect("schema.json should be valid JSON");

        // Verify it roundtrips through PulumiSchema
        let parsed: PulumiSchema =
            serde_json::from_str(json_str).expect("should parse as PulumiSchema");
        assert_eq!(parsed.name, "acme");
        assert!(!parsed.resources.is_empty());
        assert!(!parsed.functions.is_empty());
    }

    #[test]
    fn generate_all_produces_schema_json_with_all_resources() {
        let backend = PulumiBackend::new();
        let provider = test_provider();
        let resources = vec![test_resource(), IacResource {
            name: "acme_auth_method".to_string(),
            description: "An auth method".to_string(),
            category: "auth".to_string(),
            crud: test_crud(),
            attributes: vec![IacAttribute {
                api_name: "method_type".to_string(),
                canonical_name: "method_type".to_string(),
                description: "Method type".to_string(),
                iac_type: IacType::String,
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
        }];
        let data_sources = vec![test_data_source()];

        let artifacts = backend
            .generate_all(&provider, &resources, &data_sources)
            .expect("generate_all should succeed");

        // Pulumi generate_resource and generate_data_source are no-ops,
        // so only generate_provider produces an artifact (schema.json)
        let schema_artifacts: Vec<_> = artifacts
            .iter()
            .filter(|a| a.kind == ArtifactKind::Schema)
            .collect();
        assert_eq!(schema_artifacts.len(), 1);
        assert_eq!(schema_artifacts[0].path, "schema.json");

        let parsed: PulumiSchema =
            serde_json::from_str(&schema_artifacts[0].content).expect("valid JSON");
        assert_eq!(parsed.resources.len(), 2);
        assert!(parsed.resources.contains_key("acme:index:StaticSecret"));
        assert!(parsed.resources.contains_key("acme:index:AuthMethod"));
        assert_eq!(parsed.functions.len(), 1);
        assert!(parsed.functions.contains_key("acme:index:getSecretValue"));
    }

    #[test]
    fn enum_with_float_underlying_type() {
        let (schema_type, _, _, enum_values) = iac_type_to_pulumi(&IacType::Enum {
            values: vec!["1.5".into(), "2.7".into()],
            underlying: Box::new(IacType::Float),
        });
        assert_eq!(schema_type.as_deref(), Some("number"));
        let vals = enum_values.unwrap();
        assert_eq!(vals[0], serde_json::json!(1.5));
        assert_eq!(vals[1], serde_json::json!(2.7));
    }

    #[test]
    fn enum_with_boolean_underlying_type() {
        let (schema_type, _, _, enum_values) = iac_type_to_pulumi(&IacType::Enum {
            values: vec!["true".into(), "false".into()],
            underlying: Box::new(IacType::Boolean),
        });
        assert_eq!(schema_type.as_deref(), Some("boolean"));
        let vals = enum_values.unwrap();
        assert_eq!(vals[0], serde_json::Value::Bool(true));
        assert_eq!(vals[1], serde_json::Value::Bool(false));
    }

    #[test]
    fn set_maps_to_array_same_as_list() {
        let (list_type, list_items, _, _) =
            iac_type_to_pulumi(&IacType::List(Box::new(IacType::String)));
        let (set_type, set_items, _, _) =
            iac_type_to_pulumi(&IacType::Set(Box::new(IacType::String)));

        assert_eq!(list_type, set_type);
        assert_eq!(list_type.as_deref(), Some("array"));
        // Both should have items
        assert!(list_items.is_some());
        assert!(set_items.is_some());
    }

    #[test]
    fn capitalize_first_edge_cases() {
        assert_eq!(capitalize_first(""), "");
        assert_eq!(capitalize_first("a"), "A");
        assert_eq!(capitalize_first("hello"), "Hello");
        assert_eq!(capitalize_first("ALLCAPS"), "ALLCAPS");
    }

    #[test]
    fn pulumi_module_from_platform_config() {
        let backend = PulumiBackend::new();
        let mut provider = test_provider();
        let mut pulumi_config = HashMap::new();
        pulumi_config.insert(
            "pulumi".to_string(),
            toml::Value::Table({
                let mut t = toml::map::Map::new();
                t.insert("module".to_string(), toml::Value::String("mymod".to_string()));
                t
            }),
        );
        provider.platform_config = pulumi_config;

        let resource = test_resource();
        let schema = backend
            .generate_schema(&provider, &[resource], &[])
            .expect("schema generation should succeed");

        assert!(
            schema.resources.contains_key("acme:mymod:StaticSecret"),
            "resource should use custom module from platform_config, got keys: {:?}",
            schema.resources.keys().collect::<Vec<_>>()
        );
    }

    // ---- Coverage gap: computed+required attribute should appear in BOTH inputs and outputs ----

    #[test]
    fn computed_and_required_field_appears_in_inputs() {
        let backend = PulumiBackend::new();
        let provider = test_provider();

        let resource = IacResource {
            name: "acme_server".to_string(),
            description: "A server".to_string(),
            category: "compute".to_string(),
            crud: test_crud(),
            attributes: vec![
                IacAttribute {
                    api_name: "name".to_string(),
                    canonical_name: "name".to_string(),
                    description: "Server name".to_string(),
                    iac_type: IacType::String,
                    required: true,
                    computed: true,
                    sensitive: false,
                    immutable: false,
                    default_value: None,
                    enum_values: None,
                    read_path: None,
                    update_only: false,
                },
                IacAttribute {
                    api_name: "ip_address".to_string(),
                    canonical_name: "ip_address".to_string(),
                    description: "Assigned IP".to_string(),
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
        };

        let schema = backend
            .generate_schema(&provider, &[resource], &[])
            .expect("schema generation should succeed");

        let res = &schema.resources["acme:index:Server"];
        assert!(
            res.input_properties.contains_key("name"),
            "computed+required 'name' must appear in input_properties"
        );
        assert!(
            res.required_inputs.contains(&"name".to_string()),
            "computed+required 'name' must be in required_inputs"
        );
        assert!(
            !res.input_properties.contains_key("ipAddress"),
            "computed-only (not required) 'ipAddress' must NOT be in input_properties"
        );
        assert!(
            res.properties.contains_key("name"),
            "computed+required 'name' must also appear in output properties"
        );
        assert!(
            res.properties.contains_key("ipAddress"),
            "computed-only 'ipAddress' must appear in output properties"
        );
        assert!(
            res.required.contains(&"name".to_string()),
            "computed+required should be in required outputs"
        );
        assert!(
            res.required.contains(&"ipAddress".to_string()),
            "computed-only should be in required outputs"
        );
    }

    // ---- Coverage gap: enum parse failure fallbacks ----

    #[test]
    fn enum_integer_unparseable_falls_back_to_string() {
        let (schema_type, _, _, enum_values) = iac_type_to_pulumi(&IacType::Enum {
            values: vec!["1".into(), "not_a_number".into(), "3".into()],
            underlying: Box::new(IacType::Integer),
        });
        assert_eq!(schema_type.as_deref(), Some("integer"));
        let vals = enum_values.unwrap();
        assert_eq!(vals[0], serde_json::json!(1));
        assert_eq!(vals[1], serde_json::Value::String("not_a_number".into()));
        assert_eq!(vals[2], serde_json::json!(3));
    }

    #[test]
    fn enum_float_unparseable_falls_back_to_string() {
        let (schema_type, _, _, enum_values) = iac_type_to_pulumi(&IacType::Enum {
            values: vec!["1.5".into(), "not_a_float".into()],
            underlying: Box::new(IacType::Float),
        });
        assert_eq!(schema_type.as_deref(), Some("number"));
        let vals = enum_values.unwrap();
        assert_eq!(vals[0], serde_json::json!(1.5));
        assert_eq!(vals[1], serde_json::Value::String("not_a_float".into()));
    }

    #[test]
    fn enum_boolean_non_bool_value_falls_back_to_string() {
        let (schema_type, _, _, enum_values) = iac_type_to_pulumi(&IacType::Enum {
            values: vec!["true".into(), "false".into(), "maybe".into(), "1".into()],
            underlying: Box::new(IacType::Boolean),
        });
        assert_eq!(schema_type.as_deref(), Some("boolean"));
        let vals = enum_values.unwrap();
        assert_eq!(vals[0], serde_json::Value::Bool(true));
        assert_eq!(vals[1], serde_json::Value::Bool(false));
        assert_eq!(vals[2], serde_json::Value::String("maybe".into()));
        assert_eq!(vals[3], serde_json::Value::String("1".into()));
    }

    #[test]
    fn enum_with_empty_values_list() {
        let (schema_type, _, _, enum_values) = iac_type_to_pulumi(&IacType::Enum {
            values: vec![],
            underlying: Box::new(IacType::String),
        });
        assert_eq!(schema_type.as_deref(), Some("string"));
        let vals = enum_values.unwrap();
        assert!(vals.is_empty());
    }

    // ---- Coverage gap: Backend no-op methods return empty vecs ----

    #[test]
    fn generate_resource_returns_empty_vec() {
        let backend = PulumiBackend::new();
        let provider = test_provider();
        let resource = test_resource();
        let result = backend
            .generate_resource(&resource, &provider)
            .expect("generate_resource should succeed");
        assert!(result.is_empty(), "Pulumi generate_resource should return empty vec");
    }

    #[test]
    fn generate_data_source_returns_empty_vec() {
        let backend = PulumiBackend::new();
        let provider = test_provider();
        let ds = test_data_source();
        let result = backend
            .generate_data_source(&ds, &provider)
            .expect("generate_data_source should succeed");
        assert!(result.is_empty(), "Pulumi generate_data_source should return empty vec");
    }

    #[test]
    fn generate_test_returns_empty_vec() {
        let backend = PulumiBackend::new();
        let provider = test_provider();
        let resource = test_resource();
        let result = backend
            .generate_test(&resource, &provider)
            .expect("generate_test should succeed");
        assert!(result.is_empty(), "Pulumi generate_test should return empty vec");
    }

    // ---- Coverage gap: provider with only token_field (no gateway) ----

    #[test]
    fn provider_with_only_token_field() {
        let backend = PulumiBackend::new();
        let provider = IacProvider {
            name: "tokenonly".to_string(),
            description: "Token-only provider".to_string(),
            version: "1.0.0".to_string(),
            auth: AuthInfo {
                token_field: "api_key".to_string(),
                env_var: "API_KEY".to_string(),
                gateway_url_field: String::new(),
                gateway_env_var: String::new(),
            },
            skip_fields: vec![],
            platform_config: HashMap::new(),
        };

        let schema = backend
            .generate_schema(&provider, &[], &[])
            .expect("schema generation should succeed");

        assert!(
            schema.provider.input_properties.contains_key("apiKey"),
            "should have apiKey"
        );
        assert!(
            !schema.provider.input_properties.contains_key(""),
            "should not have empty-string key"
        );
        assert_eq!(
            schema.provider.input_properties.len(),
            1,
            "should have exactly one input property"
        );
        let token_prop = &schema.provider.input_properties["apiKey"];
        assert_eq!(token_prop.secret, Some(true));
    }

    #[test]
    fn provider_with_only_gateway_field() {
        let backend = PulumiBackend::new();
        let provider = IacProvider {
            name: "gatewayonly".to_string(),
            description: "Gateway-only provider".to_string(),
            version: "1.0.0".to_string(),
            auth: AuthInfo {
                token_field: String::new(),
                env_var: String::new(),
                gateway_url_field: "base_url".to_string(),
                gateway_env_var: "BASE_URL".to_string(),
            },
            skip_fields: vec![],
            platform_config: HashMap::new(),
        };

        let schema = backend
            .generate_schema(&provider, &[], &[])
            .expect("schema generation should succeed");

        assert_eq!(schema.provider.input_properties.len(), 1);
        assert!(schema.provider.input_properties.contains_key("baseUrl"));
        let url_prop = &schema.provider.input_properties["baseUrl"];
        assert_eq!(url_prop.secret, None, "gateway URL should not be secret");
        assert_eq!(url_prop.description.as_deref(), Some("API gateway URL"));
    }

    // ---- Coverage gap: JSON output format uses camelCase keys ----

    #[test]
    fn json_output_uses_camel_case_keys_not_snake_case() {
        let backend = PulumiBackend::new();
        let provider = test_provider();
        let resource = test_resource();
        let ds = test_data_source();

        let artifacts = backend
            .generate_provider(&provider, &[resource], &[ds])
            .expect("generate_provider should succeed");
        let json_str = &artifacts[0].content;

        assert!(json_str.contains("\"inputProperties\""), "must use camelCase inputProperties");
        assert!(json_str.contains("\"requiredInputs\""), "must use camelCase requiredInputs");
        assert!(json_str.contains("\"displayName\""), "must use camelCase displayName");
        assert!(json_str.contains("\"replaceOnChanges\""), "must use camelCase replaceOnChanges");
        assert!(!json_str.contains("\"input_properties\""), "snake_case must not appear");
        assert!(!json_str.contains("\"required_inputs\""), "snake_case must not appear");
        assert!(!json_str.contains("\"display_name\""), "snake_case must not appear");
        assert!(!json_str.contains("\"replace_on_changes\""), "snake_case must not appear");
    }

    #[test]
    fn json_output_omits_empty_optional_fields() {
        let backend = PulumiBackend::new();
        let provider = IacProvider {
            name: "minimal".to_string(),
            description: String::new(),
            version: "0.0.1".to_string(),
            auth: AuthInfo {
                token_field: String::new(),
                env_var: String::new(),
                gateway_url_field: String::new(),
                gateway_env_var: String::new(),
            },
            skip_fields: vec![],
            platform_config: HashMap::new(),
        };

        let artifacts = backend
            .generate_provider(&provider, &[], &[])
            .expect("generate_provider should succeed");
        let json_val: serde_json::Value = serde_json::from_str(&artifacts[0].content).unwrap();
        let obj = json_val.as_object().unwrap();

        assert!(!obj.contains_key("homepage"), "None homepage should be omitted");
        assert!(!obj.contains_key("repository"), "None repository should be omitted");
        assert!(!obj.contains_key("publisher"), "None publisher should be omitted");
        assert!(!obj.contains_key("resources"), "empty resources map should be omitted");
        assert!(!obj.contains_key("functions"), "empty functions map should be omitted");
        assert!(!obj.contains_key("types"), "empty types map should be omitted");
    }

    // ---- Coverage gap: deeply nested and compound types ----

    #[test]
    fn map_of_list_of_integer_type_mapping() {
        let (schema_type, _, addl_props, _) = iac_type_to_pulumi(
            &IacType::Map(Box::new(IacType::List(Box::new(IacType::Integer)))),
        );
        assert_eq!(schema_type.as_deref(), Some("object"));
        let addl = addl_props.unwrap();
        assert_eq!(addl.schema_type.as_deref(), Some("array"));
        let items = addl.items.unwrap();
        assert_eq!(items.schema_type.as_deref(), Some("integer"));
    }

    #[test]
    fn map_of_map_of_string_type_mapping() {
        let (schema_type, _, addl_props, _) = iac_type_to_pulumi(
            &IacType::Map(Box::new(IacType::Map(Box::new(IacType::String)))),
        );
        assert_eq!(schema_type.as_deref(), Some("object"));
        let outer_addl = addl_props.unwrap();
        assert_eq!(outer_addl.schema_type.as_deref(), Some("object"));
        let inner_addl = outer_addl.additional_properties.unwrap();
        assert_eq!(inner_addl.schema_type.as_deref(), Some("string"));
    }

    #[test]
    fn list_of_enum_type_mapping() {
        let (schema_type, items, _, _) = iac_type_to_pulumi(&IacType::List(Box::new(
            IacType::Enum {
                values: vec!["a".into(), "b".into()],
                underlying: Box::new(IacType::String),
            },
        )));
        assert_eq!(schema_type.as_deref(), Some("array"));
        let inner = items.unwrap();
        assert_eq!(inner.schema_type.as_deref(), Some("string"));
        assert!(inner.enum_values.is_some());
        assert_eq!(inner.enum_values.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn map_of_boolean_type_mapping() {
        let (schema_type, _, addl_props, _) =
            iac_type_to_pulumi(&IacType::Map(Box::new(IacType::Boolean)));
        assert_eq!(schema_type.as_deref(), Some("object"));
        let addl = addl_props.unwrap();
        assert_eq!(addl.schema_type.as_deref(), Some("boolean"));
    }

    #[test]
    fn set_of_boolean_type_mapping() {
        let (schema_type, items, _, _) =
            iac_type_to_pulumi(&IacType::Set(Box::new(IacType::Boolean)));
        assert_eq!(schema_type.as_deref(), Some("array"));
        let inner = items.unwrap();
        assert_eq!(inner.schema_type.as_deref(), Some("boolean"));
    }

    // ---- Coverage gap: multiple resources produce deterministic ordering ----

    #[test]
    fn multiple_resources_appear_in_sorted_order() {
        let backend = PulumiBackend::new();
        let provider = test_provider();

        let resources = vec![
            IacResource {
                name: "acme_zebra".to_string(),
                description: "Zebra resource".to_string(),
                category: "test".to_string(),
                crud: test_crud(),
                attributes: vec![],
                identity: test_identity(),
            },
            IacResource {
                name: "acme_alpha".to_string(),
                description: "Alpha resource".to_string(),
                category: "test".to_string(),
                crud: test_crud(),
                attributes: vec![],
                identity: test_identity(),
            },
            IacResource {
                name: "acme_middle".to_string(),
                description: "Middle resource".to_string(),
                category: "test".to_string(),
                crud: test_crud(),
                attributes: vec![],
                identity: test_identity(),
            },
        ];

        let schema = backend
            .generate_schema(&provider, &resources, &[])
            .expect("schema generation should succeed");

        assert_eq!(schema.resources.len(), 3);
        let keys: Vec<_> = schema.resources.keys().collect();
        assert_eq!(keys[0], "acme:index:Alpha");
        assert_eq!(keys[1], "acme:index:Middle");
        assert_eq!(keys[2], "acme:index:Zebra");
    }

    // ---- Coverage gap: multiple data sources ordering ----

    #[test]
    fn multiple_data_sources_appear_in_sorted_order() {
        let backend = PulumiBackend::new();
        let provider = test_provider();

        let data_sources = vec![
            IacDataSource {
                name: "acme_z_info".to_string(),
                description: "Z info".to_string(),
                read_endpoint: "GET /z".to_string(),
                read_schema: "GetZ".to_string(),
                read_response_schema: None,
                attributes: vec![],
            },
            IacDataSource {
                name: "acme_a_info".to_string(),
                description: "A info".to_string(),
                read_endpoint: "GET /a".to_string(),
                read_schema: "GetA".to_string(),
                read_response_schema: None,
                attributes: vec![],
            },
        ];

        let schema = backend
            .generate_schema(&provider, &[], &data_sources)
            .expect("schema generation should succeed");

        let keys: Vec<_> = schema.functions.keys().collect();
        assert_eq!(keys[0], "acme:index:getAInfo");
        assert_eq!(keys[1], "acme:index:getZInfo");
    }

    // ---- Coverage gap: attribute with default values of various types ----

    #[test]
    fn attribute_with_string_default_value() {
        let attr = IacAttribute {
            api_name: "region".to_string(),
            canonical_name: "region".to_string(),
            description: "Region".to_string(),
            iac_type: IacType::String,
            required: false,
            computed: false,
            sensitive: false,
            immutable: false,
            default_value: Some(serde_json::json!("us-east-1")),
            enum_values: None,
            read_path: None,
            update_only: false,
        };
        let prop = PropertySpec::from(&attr);
        assert_eq!(prop.default, Some(serde_json::json!("us-east-1")));
        assert_eq!(prop.schema_type.as_deref(), Some("string"));
    }

    #[test]
    fn attribute_with_integer_default_value() {
        let attr = IacAttribute {
            api_name: "port".to_string(),
            canonical_name: "port".to_string(),
            description: "Port".to_string(),
            iac_type: IacType::Integer,
            required: false,
            computed: false,
            sensitive: false,
            immutable: false,
            default_value: Some(serde_json::json!(8080)),
            enum_values: None,
            read_path: None,
            update_only: false,
        };
        let prop = PropertySpec::from(&attr);
        assert_eq!(prop.default, Some(serde_json::json!(8080)));
    }

    #[test]
    fn attribute_with_null_default_value() {
        let attr = IacAttribute {
            api_name: "opt".to_string(),
            canonical_name: "opt".to_string(),
            description: "Optional".to_string(),
            iac_type: IacType::String,
            required: false,
            computed: false,
            sensitive: false,
            immutable: false,
            default_value: Some(serde_json::Value::Null),
            enum_values: None,
            read_path: None,
            update_only: false,
        };
        let prop = PropertySpec::from(&attr);
        assert_eq!(prop.default, Some(serde_json::Value::Null));
    }

    // ---- Coverage gap: attribute with empty description ----

    #[test]
    fn attribute_with_empty_description_maps_to_none() {
        let attr = IacAttribute {
            api_name: "field".to_string(),
            canonical_name: "field".to_string(),
            description: String::new(),
            iac_type: IacType::String,
            required: false,
            computed: false,
            sensitive: false,
            immutable: false,
            default_value: None,
            enum_values: None,
            read_path: None,
            update_only: false,
        };
        let prop = PropertySpec::from(&attr);
        assert!(prop.description.is_none(), "empty description should become None");
    }

    #[test]
    fn attribute_with_nonempty_description_maps_to_some() {
        let attr = IacAttribute {
            api_name: "field".to_string(),
            canonical_name: "field".to_string(),
            description: "A field".to_string(),
            iac_type: IacType::String,
            required: false,
            computed: false,
            sensitive: false,
            immutable: false,
            default_value: None,
            enum_values: None,
            read_path: None,
            update_only: false,
        };
        let prop = PropertySpec::from(&attr);
        assert_eq!(prop.description.as_deref(), Some("A field"));
    }

    // ---- Coverage gap: PropertySpec::from(&IacType) as standalone conversion ----

    #[test]
    fn property_spec_from_iac_type_returns_clean_property() {
        let prop = PropertySpec::from(&IacType::String);
        assert_eq!(prop.schema_type.as_deref(), Some("string"));
        assert!(prop.description.is_none());
        assert!(prop.secret.is_none());
        assert!(prop.default.is_none());
        assert!(prop.items.is_none());
        assert!(prop.additional_properties.is_none());
        assert!(prop.ref_path.is_none());
        assert!(prop.replace_on_changes.is_none());
        assert!(prop.enum_values.is_none());
    }

    #[test]
    fn property_spec_from_iac_type_preserves_nested_structure() {
        let prop = PropertySpec::from(&IacType::Map(Box::new(IacType::List(
            Box::new(IacType::Float),
        ))));
        assert_eq!(prop.schema_type.as_deref(), Some("object"));
        let addl = prop.additional_properties.unwrap();
        assert_eq!(addl.schema_type.as_deref(), Some("array"));
        let items = addl.items.unwrap();
        assert_eq!(items.schema_type.as_deref(), Some("number"));
    }

    // ---- Coverage gap: data source with empty description ----

    #[test]
    fn data_source_empty_description_maps_to_none() {
        let backend = PulumiBackend::new();
        let provider = test_provider();

        let ds = IacDataSource {
            name: "acme_lookup".to_string(),
            description: String::new(),
            read_endpoint: "GET /lookup".to_string(),
            read_schema: "LookupReq".to_string(),
            read_response_schema: None,
            attributes: vec![],
        };

        let schema = backend
            .generate_schema(&provider, &[], &[ds])
            .expect("schema generation should succeed");

        let func = &schema.functions["acme:index:getLookup"];
        assert!(func.description.is_none(), "empty data source description should be None");
    }

    // ---- Coverage gap: naming convention edge cases ----

    #[test]
    fn naming_resource_type_strips_provider_prefix() {
        let naming = PulumiNaming;
        assert_eq!(naming.resource_type_name("acme_my_resource", "acme"), "MyResource");
    }

    #[test]
    fn naming_resource_type_no_prefix_match() {
        let naming = PulumiNaming;
        let result = naming.resource_type_name("other_resource", "acme");
        assert!(!result.is_empty(), "should produce a non-empty name even without prefix match");
    }

    #[test]
    fn naming_file_name_ignores_resource_name() {
        let naming = PulumiNaming;
        assert_eq!(naming.file_name("anything", &ArtifactKind::Schema), "schema.json");
        assert_eq!(naming.file_name("", &ArtifactKind::Schema), "schema.json");
        assert_eq!(naming.file_name("complex_name", &ArtifactKind::Resource), "schema.json");
    }

    #[test]
    fn naming_field_name_converts_to_camel_case() {
        let naming = PulumiNaming;
        assert_eq!(naming.field_name("my_field_name"), "myFieldName");
        assert_eq!(naming.field_name("single"), "single");
        assert_eq!(naming.field_name("UPPER_CASE"), "uPPERCASE");
    }

    // ---- Coverage gap: provider description stored even when empty ----

    #[test]
    fn provider_empty_description_stored_as_some_empty_string() {
        let backend = PulumiBackend::new();
        let provider = IacProvider {
            name: "test".to_string(),
            description: String::new(),
            version: "0.0.1".to_string(),
            auth: AuthInfo {
                token_field: String::new(),
                env_var: String::new(),
                gateway_url_field: String::new(),
                gateway_env_var: String::new(),
            },
            skip_fields: vec![],
            platform_config: HashMap::new(),
        };

        let schema = backend
            .generate_schema(&provider, &[], &[])
            .expect("schema generation should succeed");

        assert_eq!(
            schema.description,
            Some(String::new()),
            "provider description is stored as Some even when empty"
        );
        assert_eq!(
            schema.provider.description,
            Some(String::new()),
            "provider resource description is stored as Some even when empty"
        );
    }

    // ---- Coverage gap: data source computed attributes are required outputs, non-computed not ----

    #[test]
    fn data_source_non_required_non_computed_field_not_in_output_required() {
        let backend = PulumiBackend::new();
        let provider = test_provider();

        let ds = IacDataSource {
            name: "acme_info".to_string(),
            description: "Info".to_string(),
            read_endpoint: "GET /info".to_string(),
            read_schema: "GetInfo".to_string(),
            read_response_schema: None,
            attributes: vec![
                IacAttribute {
                    api_name: "filter".to_string(),
                    canonical_name: "filter".to_string(),
                    description: "Filter param".to_string(),
                    iac_type: IacType::String,
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
                    api_name: "result".to_string(),
                    canonical_name: "result".to_string(),
                    description: "Result".to_string(),
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
        };

        let schema = backend
            .generate_schema(&provider, &[], &[ds])
            .expect("schema generation should succeed");

        let func = &schema.functions["acme:index:getInfo"];
        let outputs = func.outputs.as_ref().unwrap();
        assert!(
            outputs.required.contains(&"result".to_string()),
            "computed field should be in output required"
        );
        assert!(
            !outputs.required.contains(&"filter".to_string()),
            "non-computed field should NOT be in output required"
        );
    }

    // ---- Coverage gap: resource required+non-computed in required outputs ----

    #[test]
    fn required_non_computed_field_in_required_outputs() {
        let backend = PulumiBackend::new();
        let provider = test_provider();

        let resource = IacResource {
            name: "acme_item".to_string(),
            description: "Item".to_string(),
            category: "test".to_string(),
            crud: test_crud(),
            attributes: vec![
                IacAttribute {
                    api_name: "name".to_string(),
                    canonical_name: "name".to_string(),
                    description: "Name".to_string(),
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
                    api_name: "opt".to_string(),
                    canonical_name: "opt".to_string(),
                    description: "Optional".to_string(),
                    iac_type: IacType::String,
                    required: false,
                    computed: false,
                    sensitive: false,
                    immutable: false,
                    default_value: None,
                    enum_values: None,
                    read_path: None,
                    update_only: false,
                },
            ],
            identity: test_identity(),
        };

        let schema = backend
            .generate_schema(&provider, &[resource], &[])
            .expect("schema generation should succeed");

        let res = &schema.resources["acme:index:Item"];
        assert!(
            res.required.contains(&"name".to_string()),
            "required field should be in required outputs"
        );
        assert!(
            !res.required.contains(&"opt".to_string()),
            "optional non-computed field should NOT be in required outputs"
        );
    }

    // ---- Coverage gap: enum with nested List underlying type ----

    #[test]
    fn enum_with_list_underlying_preserves_base_type() {
        let (schema_type, items, _, enum_values) = iac_type_to_pulumi(&IacType::Enum {
            values: vec!["a".into(), "b".into()],
            underlying: Box::new(IacType::List(Box::new(IacType::String))),
        });
        assert_eq!(schema_type.as_deref(), Some("array"));
        assert!(items.is_none(), "enum branch does not propagate items from underlying");
        assert!(enum_values.is_some());
        let vals = enum_values.unwrap();
        assert_eq!(vals[0], serde_json::Value::String("a".into()));
    }

    // ---- Coverage gap: capitalize_first with Unicode ----

    #[test]
    fn capitalize_first_unicode() {
        assert_eq!(capitalize_first("über"), "Über");
        assert_eq!(capitalize_first("日本語"), "日本語");
    }

    // ---- Coverage gap: sensitive + immutable combined on same attribute ----

    #[test]
    fn sensitive_and_immutable_both_set() {
        let attr = IacAttribute {
            api_name: "api_key".to_string(),
            canonical_name: "api_key".to_string(),
            description: "API key".to_string(),
            iac_type: IacType::String,
            required: true,
            computed: false,
            sensitive: true,
            immutable: true,
            default_value: None,
            enum_values: None,
            read_path: None,
            update_only: false,
        };
        let prop = PropertySpec::from(&attr);
        assert_eq!(prop.secret, Some(true));
        assert_eq!(prop.replace_on_changes, Some(true));
    }

    // ---- Coverage gap: non-sensitive, non-immutable attribute has None for those fields ----

    #[test]
    fn non_sensitive_non_immutable_has_none_flags() {
        let attr = IacAttribute {
            api_name: "label".to_string(),
            canonical_name: "label".to_string(),
            description: "Label".to_string(),
            iac_type: IacType::String,
            required: false,
            computed: false,
            sensitive: false,
            immutable: false,
            default_value: None,
            enum_values: None,
            read_path: None,
            update_only: false,
        };
        let prop = PropertySpec::from(&attr);
        assert!(prop.secret.is_none());
        assert!(prop.replace_on_changes.is_none());
    }

    // ---- Coverage gap: Object type always produces type=object, no items/additionalProperties ----

    #[test]
    fn object_type_has_no_items_or_additional_properties() {
        let (schema_type, items, addl, enum_vals) = iac_type_to_pulumi(&IacType::Object {
            name: "Nested".to_string(),
            fields: vec![],
        });
        assert_eq!(schema_type.as_deref(), Some("object"));
        assert!(items.is_none());
        assert!(addl.is_none());
        assert!(enum_vals.is_none());
    }

    // ---- Coverage gap: Any type maps to string ----

    #[test]
    fn any_type_maps_to_string() {
        let (schema_type, items, addl, enum_vals) = iac_type_to_pulumi(&IacType::Any);
        assert_eq!(schema_type.as_deref(), Some("string"));
        assert!(items.is_none());
        assert!(addl.is_none());
        assert!(enum_vals.is_none());
    }

    // ---- Coverage gap: platform_config with pulumi key but no module subkey defaults to "index" ----

    #[test]
    fn platform_config_pulumi_without_module_defaults_to_index() {
        let backend = PulumiBackend::new();
        let mut provider = test_provider();
        let mut pulumi_config = HashMap::new();
        pulumi_config.insert(
            "pulumi".to_string(),
            toml::Value::Table({
                let mut t = toml::map::Map::new();
                t.insert("other_key".to_string(), toml::Value::String("val".to_string()));
                t
            }),
        );
        provider.platform_config = pulumi_config;

        let resource = IacResource {
            name: "acme_thing".to_string(),
            description: "A thing".to_string(),
            category: "test".to_string(),
            crud: test_crud(),
            attributes: vec![],
            identity: test_identity(),
        };

        let schema = backend
            .generate_schema(&provider, &[resource], &[])
            .expect("schema generation should succeed");

        assert!(
            schema.resources.contains_key("acme:index:Thing"),
            "should default to 'index' module when pulumi config has no 'module' key"
        );
    }

    // ---- Coverage gap: platform_config with non-string module value defaults to "index" ----

    #[test]
    fn platform_config_pulumi_module_non_string_defaults_to_index() {
        let backend = PulumiBackend::new();
        let mut provider = test_provider();
        let mut pulumi_config = HashMap::new();
        pulumi_config.insert(
            "pulumi".to_string(),
            toml::Value::Table({
                let mut t = toml::map::Map::new();
                t.insert("module".to_string(), toml::Value::Integer(42));
                t
            }),
        );
        provider.platform_config = pulumi_config;

        let resource = IacResource {
            name: "acme_thing".to_string(),
            description: "A thing".to_string(),
            category: "test".to_string(),
            crud: test_crud(),
            attributes: vec![],
            identity: test_identity(),
        };

        let schema = backend
            .generate_schema(&provider, &[resource], &[])
            .expect("schema generation should succeed");

        assert!(
            schema.resources.contains_key("acme:index:Thing"),
            "non-string module value should fallback to 'index'"
        );
    }

    // ---- Coverage gap: generate_provider artifact kind is Schema ----

    #[test]
    fn generate_provider_artifact_kind_is_schema() {
        let backend = PulumiBackend::new();
        let provider = test_provider();

        let artifacts = backend
            .generate_provider(&provider, &[], &[])
            .expect("generate_provider should succeed");

        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::Schema);
        assert_eq!(artifacts[0].path, "schema.json");
    }

    // ---- Coverage gap: large number of enum values ----

    #[test]
    fn enum_with_many_values() {
        let values: Vec<String> = (0..100).map(|i| format!("val_{i}")).collect();
        let (schema_type, _, _, enum_values) = iac_type_to_pulumi(&IacType::Enum {
            values,
            underlying: Box::new(IacType::String),
        });
        assert_eq!(schema_type.as_deref(), Some("string"));
        let vals = enum_values.unwrap();
        assert_eq!(vals.len(), 100);
        assert_eq!(vals[0], serde_json::Value::String("val_0".into()));
        assert_eq!(vals[99], serde_json::Value::String("val_99".into()));
    }

    // ---- Coverage gap: display_name capitalize_first integration ----

    #[test]
    fn display_name_is_capitalized_provider_name() {
        let backend = PulumiBackend::new();
        let mut provider = test_provider();
        provider.name = "mycloud".to_string();

        let schema = backend
            .generate_schema(&provider, &[], &[])
            .expect("schema generation should succeed");

        assert_eq!(schema.display_name.as_deref(), Some("Mycloud"));
    }

    // ---- Coverage gap: language section uses provider name in package paths ----

    #[test]
    fn language_section_uses_correct_provider_name() {
        let backend = PulumiBackend::new();
        let mut provider = test_provider();
        provider.name = "foobar".to_string();

        let schema = backend
            .generate_schema(&provider, &[], &[])
            .expect("schema generation should succeed");

        assert_eq!(schema.language["nodejs"]["packageName"], "@pulumi/foobar");
        assert_eq!(schema.language["python"]["packageName"], "pulumi_foobar");
        assert!(schema.language["go"]["importBasePath"]
            .as_str()
            .unwrap()
            .contains("pulumi-foobar"));
        assert!(schema.language["java"]["basePackage"]
            .as_str()
            .unwrap()
            .contains("foobar"));
    }

    // ---- coerce_enum_value helper tests ----

    #[test]
    fn coerce_enum_value_integer_valid() {
        let val = coerce_enum_value("42", &IacType::Integer);
        assert_eq!(val, serde_json::json!(42));
    }

    #[test]
    fn coerce_enum_value_integer_negative() {
        let val = coerce_enum_value("-7", &IacType::Integer);
        assert_eq!(val, serde_json::json!(-7));
    }

    #[test]
    fn coerce_enum_value_integer_invalid() {
        let val = coerce_enum_value("abc", &IacType::Integer);
        assert_eq!(val, serde_json::Value::String("abc".into()));
    }

    #[test]
    fn coerce_enum_value_float_valid() {
        let val = coerce_enum_value("3.14", &IacType::Float);
        assert_eq!(val, serde_json::json!(3.14));
    }

    #[test]
    fn coerce_enum_value_float_invalid() {
        let val = coerce_enum_value("nope", &IacType::Float);
        assert_eq!(val, serde_json::Value::String("nope".into()));
    }

    #[test]
    fn coerce_enum_value_boolean_true() {
        let val = coerce_enum_value("true", &IacType::Boolean);
        assert_eq!(val, serde_json::Value::Bool(true));
    }

    #[test]
    fn coerce_enum_value_boolean_false() {
        let val = coerce_enum_value("false", &IacType::Boolean);
        assert_eq!(val, serde_json::Value::Bool(false));
    }

    #[test]
    fn coerce_enum_value_boolean_invalid() {
        let val = coerce_enum_value("yes", &IacType::Boolean);
        assert_eq!(val, serde_json::Value::String("yes".into()));
    }

    #[test]
    fn coerce_enum_value_string_passthrough() {
        let val = coerce_enum_value("hello", &IacType::String);
        assert_eq!(val, serde_json::Value::String("hello".into()));
    }

    #[test]
    fn coerce_enum_value_other_type_passthrough() {
        let val = coerce_enum_value("val", &IacType::Any);
        assert_eq!(val, serde_json::Value::String("val".into()));
    }

    // ---- to_pascal_case_custom tests ----

    #[test]
    fn to_pascal_case_custom_basic() {
        assert_eq!(to_pascal_case_custom("hello_world"), "HelloWorld");
    }

    #[test]
    fn to_pascal_case_custom_hyphens() {
        assert_eq!(to_pascal_case_custom("my-resource"), "MyResource");
    }

    #[test]
    fn to_pascal_case_custom_single_word() {
        assert_eq!(to_pascal_case_custom("item"), "Item");
    }

    #[test]
    fn to_pascal_case_custom_empty() {
        assert_eq!(to_pascal_case_custom(""), "");
    }

    // ---- Full schema JSON determinism test ----

    #[test]
    fn schema_json_is_deterministic_across_runs() {
        let backend = PulumiBackend::new();
        let provider = test_provider();
        let resource = test_resource();
        let ds = test_data_source();

        let json1 = {
            let artifacts = backend
                .generate_provider(&provider, &[resource.clone()], &[ds.clone()])
                .unwrap();
            artifacts[0].content.clone()
        };
        let json2 = {
            let artifacts = backend
                .generate_provider(&provider, &[resource], &[ds])
                .unwrap();
            artifacts[0].content.clone()
        };

        assert_eq!(json1, json2, "schema.json output must be deterministic");
    }

    // ---- Data source function description preserved ----

    #[test]
    fn data_source_description_preserved_in_function() {
        let backend = PulumiBackend::new();
        let provider = test_provider();
        let ds = test_data_source();

        let schema = backend
            .generate_schema(&provider, &[], &[ds])
            .unwrap();

        let func = &schema.functions["acme:index:getSecretValue"];
        assert_eq!(func.description.as_deref(), Some("Read a secret value"));
    }

    // ---- Resource description preserved ----

    #[test]
    fn resource_description_preserved() {
        let backend = PulumiBackend::new();
        let provider = test_provider();
        let resource = test_resource();

        let schema = backend
            .generate_schema(&provider, &[resource], &[])
            .unwrap();

        let res = &schema.resources["acme:index:StaticSecret"];
        assert_eq!(res.description.as_deref(), Some("A static secret resource"));
    }

    // ---- Config variables have correct property types ----

    #[test]
    fn config_variables_have_correct_schema_type() {
        let backend = PulumiBackend::new();
        let provider = test_provider();

        let schema = backend
            .generate_schema(&provider, &[], &[])
            .unwrap();

        let variables = &schema.config["variables"];
        assert_eq!(variables["apiUrl"]["type"], "string");
        assert_eq!(variables["apiToken"]["type"], "string");
        assert_eq!(variables["apiToken"]["secret"], true);
    }
}
