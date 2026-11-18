//! Frozen identities of the saved reference models.
//!
//! `model_sha256` is the SHA-256 digest of the compact serialization of the typed model with
//! defaults applied. Schema version 1 promises that an existing document keeps its meaning
//! and its digest. A change to the typed model that serializes an existing document
//! differently, such as a new field that is written with its default, a renamed key or a
//! reordered field, fails here.
//!
//! Update a digest only when the reference model itself was edited on purpose. In every
//! other case the change breaks version 1 and needs a new schema version.

use caekit::{Model, solve};

const FROZEN: &[(&str, &[u8], &str)] = &[
    (
        "axial_bar",
        include_bytes!("../../../examples/axial_bar.json"),
        "ef1ad1431b563c0b252bd8f872cd40941a8feebcabcfe6692ecb529e85614e17",
    ),
    (
        "cooling_channel",
        include_bytes!("../../../examples/cooling_channel.json"),
        "7e3c9e22d7e6440124ef2cbfeec4907fdbc834435d7baf4cf22f9b22ac0388c8",
    ),
    (
        "elastic_solid",
        include_bytes!("../../../examples/elastic_solid.json"),
        "faec4995d67cbf9758d4410a0e4bdf5ff2e963fb1393f7b59d67709e786cc6f5",
    ),
    (
        "heat_conduction",
        include_bytes!("../../../examples/heat_conduction.json"),
        "92a87d6342966ab22015eb3877847fb825e7ee0b63dc2184441ba35bfc5cdb8c",
    ),
    (
        "laminar_channel",
        include_bytes!("../../../examples/laminar_channel.json"),
        "6d727104492ae5145e37696bde65b6d6c76899e90a5c1e5fafd85cc47f0669fe",
    ),
    (
        "multimaterial_heat_conduction",
        include_bytes!("../../../examples/multimaterial_heat_conduction.json"),
        "f0599ab94a8ff13831daf0929f03d585811464569351324d9f591304823b3b1d",
    ),
    (
        "thermoelastic_solid",
        include_bytes!("../../../examples/thermoelastic_solid.json"),
        "a58274be1f1e0f1b3aaf7e2ddd12b46c4a2d03b7e793320dec93257ba0145804",
    ),
];

#[test]
fn reference_models_keep_their_version_1_digest() {
    for (name, document, digest) in FROZEN {
        let model = Model::from_json(document).unwrap();
        assert_eq!(model.schema_version, 1, "{name}");
        let report = solve(&model).unwrap();
        assert_eq!(
            report.model_sha256, *digest,
            "examples/{name}.json: the serialized typed model of schema version 1 changed"
        );
    }
}

#[test]
fn the_digest_ignores_formatting_and_key_order() {
    for (name, document, digest) in FROZEN {
        let value: serde_json::Value = serde_json::from_slice(document).unwrap();
        // serde_json orders object keys alphabetically here, unlike the saved files.
        let reordered = serde_json::to_vec_pretty(&value).unwrap();
        assert_ne!(reordered.as_slice(), *document, "{name}");
        let report = solve(&Model::from_json(&reordered).unwrap()).unwrap();
        assert_eq!(report.model_sha256, *digest, "{name}");
    }
}
