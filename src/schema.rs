use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Top-level Pulumi Package Schema (schema.json).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PulumiSchema {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub config: BTreeMap<String, serde_json::Value>,
    pub provider: ProviderResource,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub resources: BTreeMap<String, ResourceSchema>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub functions: BTreeMap<String, FunctionSchema>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub types: BTreeMap<String, ComplexType>,
    pub language: serde_json::Value,
}

/// Provider resource in the schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderResource {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub input_properties: BTreeMap<String, PropertySpec>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub required_inputs: Vec<String>,
}

/// A single resource in the Pulumi schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSchema {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub input_properties: BTreeMap<String, PropertySpec>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub required_inputs: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub properties: BTreeMap<String, PropertySpec>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub required: Vec<String>,
}

/// A data source (function) in the Pulumi schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionSchema {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inputs: Option<ObjectTypeSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outputs: Option<ObjectTypeSpec>,
}

/// An object type definition (inputs/outputs).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectTypeSpec {
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub properties: BTreeMap<String, PropertySpec>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub required: Vec<String>,
}

/// Complex type definition in the types section.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComplexType {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub properties: BTreeMap<String, PropertySpec>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub required: Vec<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub schema_type: Option<String>,
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<EnumValue>>,
}

/// An enum value in a complex type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumValue {
    pub value: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A property definition in the Pulumi schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertySpec {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub schema_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<PropertySpec>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_properties: Option<Box<PropertySpec>>,
    #[serde(rename = "$ref", skip_serializing_if = "Option::is_none")]
    pub ref_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replace_on_changes: Option<bool>,
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<serde_json::Value>>,
}

impl PropertySpec {
    /// Create a simple property with only a type and all other fields `None`.
    #[must_use]
    pub fn typed(schema_type: &str) -> Self {
        Self {
            schema_type: Some(schema_type.to_owned()),
            description: None,
            secret: None,
            default: None,
            items: None,
            additional_properties: None,
            ref_path: None,
            replace_on_changes: None,
            enum_values: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_spec_ref_path_serializes_as_dollar_ref() {
        let prop = PropertySpec {
            schema_type: None,
            description: None,
            secret: None,
            default: None,
            items: None,
            additional_properties: None,
            ref_path: Some("#/types/pkg:index:MyType".to_string()),
            replace_on_changes: None,
            enum_values: None,
        };
        let json = serde_json::to_value(&prop).unwrap();
        assert!(json.get("$ref").is_some(), "$ref key must appear in JSON");
        assert_eq!(json["$ref"], "#/types/pkg:index:MyType");
        assert!(json.get("ref_path").is_none(), "ref_path must not leak into JSON");
        assert!(json.get("refPath").is_none(), "refPath must not leak into JSON");
    }

    #[test]
    fn property_spec_ref_path_deserializes_from_dollar_ref() {
        let json = serde_json::json!({"$ref": "#/types/pkg:index:MyType"});
        let prop: PropertySpec = serde_json::from_value(json).unwrap();
        assert_eq!(prop.ref_path.as_deref(), Some("#/types/pkg:index:MyType"));
    }

    #[test]
    fn property_spec_enum_serializes_as_enum_key() {
        let prop = PropertySpec {
            schema_type: Some("string".into()),
            description: None,
            secret: None,
            default: None,
            items: None,
            additional_properties: None,
            ref_path: None,
            replace_on_changes: None,
            enum_values: Some(vec![
                serde_json::Value::String("a".into()),
                serde_json::Value::String("b".into()),
            ]),
        };
        let json = serde_json::to_value(&prop).unwrap();
        assert!(json.get("enum").is_some(), "enum key must appear in JSON");
        assert!(json.get("enum_values").is_none(), "enum_values must not leak");
        assert!(json.get("enumValues").is_none(), "enumValues must not leak");
    }

    #[test]
    fn property_spec_none_fields_omitted_from_json() {
        let prop = PropertySpec {
            schema_type: Some("string".into()),
            description: None,
            secret: None,
            default: None,
            items: None,
            additional_properties: None,
            ref_path: None,
            replace_on_changes: None,
            enum_values: None,
        };
        let json = serde_json::to_value(&prop).unwrap();
        let obj = json.as_object().unwrap();
        assert_eq!(obj.len(), 1, "only 'type' should be present, got keys: {:?}", obj.keys().collect::<Vec<_>>());
        assert!(obj.contains_key("type"));
    }

    #[test]
    fn resource_schema_empty_collections_omitted() {
        let res = ResourceSchema {
            description: None,
            input_properties: BTreeMap::new(),
            required_inputs: vec![],
            properties: BTreeMap::new(),
            required: vec![],
        };
        let json = serde_json::to_value(&res).unwrap();
        let obj = json.as_object().unwrap();
        assert!(obj.is_empty(), "empty ResourceSchema should serialize to empty object, got: {:?}", obj);
    }

    #[test]
    fn resource_schema_roundtrip_with_camel_case() {
        let mut props = BTreeMap::new();
        props.insert("myProp".to_string(), PropertySpec {
            schema_type: Some("string".into()),
            description: Some("A property".into()),
            secret: None,
            default: None,
            items: None,
            additional_properties: None,
            ref_path: None,
            replace_on_changes: Some(true),
            enum_values: None,
        });
        let res = ResourceSchema {
            description: Some("Test resource".into()),
            input_properties: props.clone(),
            required_inputs: vec!["myProp".into()],
            properties: props,
            required: vec!["myProp".into()],
        };
        let json_str = serde_json::to_string(&res).unwrap();
        assert!(json_str.contains("inputProperties"), "should use camelCase key inputProperties");
        assert!(json_str.contains("requiredInputs"), "should use camelCase key requiredInputs");
        assert!(json_str.contains("replaceOnChanges"), "should use camelCase key replaceOnChanges");
        assert!(!json_str.contains("input_properties"), "snake_case must not leak");

        let roundtripped: ResourceSchema = serde_json::from_str(&json_str).unwrap();
        assert_eq!(roundtripped.description.as_deref(), Some("Test resource"));
        assert!(roundtripped.input_properties.contains_key("myProp"));
        assert_eq!(roundtripped.required_inputs, vec!["myProp"]);
    }

    #[test]
    fn function_schema_roundtrip() {
        let func = FunctionSchema {
            description: Some("Get data".into()),
            inputs: Some(ObjectTypeSpec {
                properties: BTreeMap::new(),
                required: vec![],
            }),
            outputs: None,
        };
        let json_str = serde_json::to_string(&func).unwrap();
        let roundtripped: FunctionSchema = serde_json::from_str(&json_str).unwrap();
        assert_eq!(roundtripped.description.as_deref(), Some("Get data"));
        assert!(roundtripped.inputs.is_some());
        assert!(roundtripped.outputs.is_none());
    }

    #[test]
    fn provider_resource_empty_collections_omitted() {
        let prov = ProviderResource {
            description: None,
            input_properties: BTreeMap::new(),
            required_inputs: vec![],
        };
        let json = serde_json::to_value(&prov).unwrap();
        let obj = json.as_object().unwrap();
        assert!(obj.is_empty(), "empty ProviderResource should serialize to empty object");
    }

    #[test]
    fn complex_type_with_enum_values_roundtrip() {
        let ct = ComplexType {
            description: Some("Status enum".into()),
            properties: BTreeMap::new(),
            required: vec![],
            schema_type: Some("string".into()),
            enum_values: Some(vec![
                EnumValue {
                    value: serde_json::Value::String("active".into()),
                    name: Some("Active".into()),
                    description: Some("Active status".into()),
                },
                EnumValue {
                    value: serde_json::Value::String("inactive".into()),
                    name: None,
                    description: None,
                },
            ]),
        };
        let json_str = serde_json::to_string(&ct).unwrap();
        assert!(json_str.contains("\"enum\""), "enum_values should serialize as 'enum' key");
        assert!(json_str.contains("\"type\""), "schema_type should serialize as 'type' key");

        let roundtripped: ComplexType = serde_json::from_str(&json_str).unwrap();
        assert_eq!(roundtripped.enum_values.as_ref().unwrap().len(), 2);
        assert_eq!(roundtripped.enum_values.as_ref().unwrap()[0].name.as_deref(), Some("Active"));
        assert!(roundtripped.enum_values.as_ref().unwrap()[1].name.is_none());
    }

    #[test]
    fn pulumi_schema_minimal_roundtrip() {
        let schema = PulumiSchema {
            name: "test".into(),
            display_name: None,
            version: "0.1.0".into(),
            description: None,
            homepage: None,
            repository: None,
            publisher: None,
            config: BTreeMap::new(),
            provider: ProviderResource {
                description: None,
                input_properties: BTreeMap::new(),
                required_inputs: vec![],
            },
            resources: BTreeMap::new(),
            functions: BTreeMap::new(),
            types: BTreeMap::new(),
            language: serde_json::json!({}),
        };
        let json_str = serde_json::to_string_pretty(&schema).unwrap();
        let roundtripped: PulumiSchema = serde_json::from_str(&json_str).unwrap();
        assert_eq!(roundtripped.name, "test");
        assert_eq!(roundtripped.version, "0.1.0");
        assert!(roundtripped.display_name.is_none());
        assert!(roundtripped.resources.is_empty());
        assert!(roundtripped.functions.is_empty());
        assert!(roundtripped.types.is_empty());
        assert!(roundtripped.config.is_empty());

        let json_val: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        let obj = json_val.as_object().unwrap();
        assert!(!obj.contains_key("displayName"), "None displayName should be omitted");
        assert!(!obj.contains_key("description"), "None description should be omitted");
        assert!(!obj.contains_key("homepage"), "None homepage should be omitted");
        assert!(!obj.contains_key("resources"), "empty resources should be omitted");
        assert!(!obj.contains_key("functions"), "empty functions should be omitted");
        assert!(!obj.contains_key("types"), "empty types should be omitted");
        assert!(!obj.contains_key("config"), "empty config should be omitted");
    }

    #[test]
    fn property_spec_with_nested_items_roundtrip() {
        let prop = PropertySpec {
            schema_type: Some("array".into()),
            description: None,
            secret: None,
            default: None,
            items: Some(Box::new(PropertySpec {
                schema_type: Some("object".into()),
                description: None,
                secret: None,
                default: None,
                items: None,
                additional_properties: Some(Box::new(PropertySpec {
                    schema_type: Some("integer".into()),
                    description: None,
                    secret: None,
                    default: None,
                    items: None,
                    additional_properties: None,
                    ref_path: None,
                    replace_on_changes: None,
                    enum_values: None,
                })),
                ref_path: None,
                replace_on_changes: None,
                enum_values: None,
            })),
            additional_properties: None,
            ref_path: None,
            replace_on_changes: None,
            enum_values: None,
        };
        let json_str = serde_json::to_string(&prop).unwrap();
        let roundtripped: PropertySpec = serde_json::from_str(&json_str).unwrap();
        assert_eq!(roundtripped.schema_type.as_deref(), Some("array"));
        let items = roundtripped.items.unwrap();
        assert_eq!(items.schema_type.as_deref(), Some("object"));
        let addl = items.additional_properties.unwrap();
        assert_eq!(addl.schema_type.as_deref(), Some("integer"));
    }

    #[test]
    fn property_spec_default_value_roundtrip() {
        let prop = PropertySpec {
            schema_type: Some("string".into()),
            description: None,
            secret: None,
            default: Some(serde_json::json!("hello")),
            items: None,
            additional_properties: None,
            ref_path: None,
            replace_on_changes: None,
            enum_values: None,
        };
        let json_str = serde_json::to_string(&prop).unwrap();
        let roundtripped: PropertySpec = serde_json::from_str(&json_str).unwrap();
        assert_eq!(roundtripped.default, Some(serde_json::json!("hello")));
    }
}
