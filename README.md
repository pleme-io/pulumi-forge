# pulumi-forge

Pulumi provider code generator from IaC forge IR.

This crate translates the `iac-forge` intermediate representation into a
Pulumi Package Schema (`schema.json`).

Part of the pleme-io code-generation pipeline:

```
sekkei -> takumi -> openapi-forge / iac-forge -> backend renderers
```

## License

MIT — see [LICENSE](./LICENSE).
