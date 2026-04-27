# pulumi-forge

> **★★★ CSE / Knowable Construction.** This repo operates under **Constructive Substrate Engineering** — canonical specification at [`pleme-io/theory/CONSTRUCTIVE-SUBSTRATE-ENGINEERING.md`](https://github.com/pleme-io/theory/blob/main/CONSTRUCTIVE-SUBSTRATE-ENGINEERING.md). The Compounding Directive (operational rules: solve once, load-bearing fixes only, idiom-first, models stay current, direction beats velocity) is in the org-level pleme-io/CLAUDE.md ★★★ section. Read both before non-trivial changes.


Pulumi provider code generator. Implements `iac_forge::Backend` to produce
Pulumi Package Schema (`schema.json`) from the iac-forge IR.

## Architecture

Takes `IacResource`, `IacDataSource`, and `IacProvider` from iac-forge IR and
generates a complete `schema.json` file conforming to the Pulumi Package Schema
specification. Resources become schema resources; data sources become functions.

## Key Types

- `PulumiBackend` -- implements `iac_forge::Backend` trait
- `PulumiSchema` -- top-level Pulumi Package Schema (name, version, resources, functions, types, language configs)
- `ResourceSchema` -- single resource definition (inputProperties, requiredInputs, properties, required)
- `FunctionSchema` -- data source definition (inputs, outputs as ObjectTypeSpec)
- `PropertySpec` -- property definition (type, description, secret, default, items, additionalProperties, enum, $ref, replaceOnChanges)
- `ProviderResource` -- provider configuration (inputProperties, requiredInputs)
- `ComplexType` -- object/enum type in the `types` section
- `ObjectTypeSpec` -- reusable object definition (properties + required)
- `EnumValue` -- enum variant (value, name, description)

## Type Mappings

```
IacType::String       -> { "type": "string" }
IacType::Integer      -> { "type": "integer" }
IacType::Float        -> { "type": "number" }
IacType::Boolean      -> { "type": "boolean" }
IacType::List(T)      -> { "type": "array", "items": <T> }
IacType::Set(T)       -> { "type": "array", "items": <T> }  (uniqueItems marker)
IacType::Map(T)       -> { "type": "object", "additionalProperties": <T> }
IacType::Object       -> $ref to types section
IacType::Enum         -> enum constraint on underlying type
IacType::Any          -> { "$ref": "pulumi.json#/Any" }
```

## Language Configs

The generated `schema.json` includes language-specific configuration for all 5 targets:
- **nodejs** -- packageName, packageDescription, respectSchemaVersion
- **python** -- packageName, pyproject, inputTypes, respectSchemaVersion
- **go** -- importBasePath, generateResourceContainerTypes, respectSchemaVersion
- **csharp** -- packageReferences, rootNamespace, respectSchemaVersion
- **java** -- basePackage, buildFiles, gradle, Maven dependencies

## Source Layout

```
src/
  lib.rs        # Public API re-exports (PulumiBackend, PulumiSchema)
  backend.rs    # Backend trait implementation + schema generation logic
  schema.rs     # Schema type definitions (serde Serialize/Deserialize)
```

## Usage

```rust
use pulumi_forge::PulumiBackend;
use iac_forge::Backend;

let backend = PulumiBackend::new();
let artifacts = backend.generate_resource(&resource, &provider)?;

// Or generate complete schema with all resources at once:
let schema = backend.generate_schema(&provider, &resources, &data_sources)?;
let json = serde_json::to_string_pretty(&schema)?;
```

## Naming Conventions

- Resource type tokens: `{provider}:{module}:{PascalCaseName}`
- Function tokens: `{provider}:{module}:get{PascalCaseName}`
- Property names: camelCase
- File output: `schema.json`

## Testing

Run: `cargo test`
