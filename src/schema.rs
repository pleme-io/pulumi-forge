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
