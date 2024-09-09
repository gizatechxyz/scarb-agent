use crate::{fsx, ProjectConfig};
use anyhow::Result;
use camino::Utf8PathBuf;
use indoc::{formatdoc, indoc};
use once_cell::sync::Lazy;
use scarb::core::{Config, PackageName};
use scarb::ops;
use serde_json::json;

const CAIRO_SOURCE_PATH: Lazy<Utf8PathBuf> = Lazy::new(|| ["src", "lib.cairo"].iter().collect());
const CAIRO_MANIFEST_PATH: Lazy<Utf8PathBuf> = Lazy::new(|| ["Scarb.toml"].iter().collect());
const PROTO_SOURCE_PATH: Lazy<Utf8PathBuf> =
    Lazy::new(|| ["proto", "oracle.proto"].iter().collect());
const ORION_PROTO_SOURCE_PATH: Lazy<Utf8PathBuf> =
    Lazy::new(|| ["proto", "orion.proto"].iter().collect());
const SERVERS_JSON_PATH: Lazy<Utf8PathBuf> = Lazy::new(|| ["servers.json"].iter().collect());
const TOOL_VERSIONS_PATH: Lazy<Utf8PathBuf> = Lazy::new(|| [".tool-versions"].iter().collect());

pub(crate) fn mk_cairo(
    canonical_path: &Utf8PathBuf,
    name: &PackageName,
    config: &Config,
    project_config: &ProjectConfig,
) -> Result<()> {
    // Create the `Scarb.toml` file.
    let manifest_path = canonical_path.join(CAIRO_MANIFEST_PATH.as_path());
    if !manifest_path.exists() {
        fsx::create_dir_all(manifest_path.parent().unwrap())?;

        fsx::write(
            &manifest_path,
            formatdoc! {r#"
            [package]
            name = "{name}"
            version = "0.1.0"
            edition = "2023_10"

            # See more keys and their definitions at https://docs.swmansion.com/scarb/docs/reference/manifest.html

            [dependencies]

            [tool.agent]
            definitions = "proto/oracle.proto"  # required
            # cairo_output = "src"
            # oracle_lock = "Oracle.lock"
            # servers_config = "servers.json"

            [cairo]
            enable-gas = false
        "#},
        )?;
    }

    // Create the `lib.cairo` file.
    let filename = canonical_path.join(CAIRO_SOURCE_PATH.as_path());
    if !filename.exists() {
        fsx::create_dir_all(filename.parent().unwrap())?;

        let lib_content = generate_lib_cairo_content(project_config);
        fsx::write(filename, lib_content)?;
    }

    // Create the `oracle.proto` file with custom content
    let filename = canonical_path.join(PROTO_SOURCE_PATH.as_path());
    if !filename.exists() {
        fsx::create_dir_all(filename.parent().unwrap())?;

        let proto_content = generate_proto_content(project_config);
        fsx::write(filename, proto_content)?;
    }

    // Create the `orion.proto` file.
    let filename = canonical_path.join(ORION_PROTO_SOURCE_PATH.as_path());
    if !filename.exists() {
        fsx::create_dir_all(filename.parent().unwrap())?;

        fsx::write(
            filename,
            indoc! {r#"
                syntax = "proto3";

                package orion;

                message F64 {
                    int64 d = 1;
                }                
            "#},
        )?;
    }

    // Create the `servers.json` file with custom content
    let filename = canonical_path.join(SERVERS_JSON_PATH.as_path());
    if !filename.exists() {
        fsx::create_dir_all(filename.parent().unwrap())?;

        let servers_content = generate_servers_json_content(project_config);
        fsx::write(filename, servers_content)?;
    }

    // Create the `InputsSchema.txt` file
    let filename = canonical_path.join("InputsSchema.txt");
    if !filename.exists() {
        fsx::create_dir_all(filename.parent().unwrap())?;

        let inputs_schema_content = generate_inputs_schema_content();
        fsx::write(filename, inputs_schema_content)?;
    }

    // Create the `tool-versions` file.
    let filename: Utf8PathBuf = canonical_path.join(TOOL_VERSIONS_PATH.as_path());
    if !filename.exists() {
        fsx::create_dir_all(filename.parent().unwrap())?;

        fsx::write(
            &filename,
            indoc! {r#"
            scarb 2.7.0
        "#},
        )?;
    }

    if let Err(err) = ops::read_workspace(&manifest_path, config) {
        config.ui().warn(formatdoc! {r#"
            compiling this new package may not work due to invalid workspace configuration

            {err:?}
        "#})
    }

    Ok(())
}

fn generate_lib_cairo_content(project_config: &ProjectConfig) -> String {
    let mut content = String::from("mod oracle;\n\n");

    if project_config.agent_api {
        content.push_str("use oracle::{AgentsApi, ExecuteRequest, MintCallData};\n");
    }

    if project_config.oracle {
        content.push_str("use oracle::{OracleRequest, OracleResponse, SqrtOracle};\n");
    }

    content.push_str(
        r#"
// Main entry point of the program.
fn main(n: i64) -> i64 {
    // Add your main logic here

"#,
    );

    if project_config.agent_api {
        content.push_str(
            r#"
    // ======== Interaction with Agents API =======
    // The Agents API allows interaction with smart contracts during the execution of a Cairo
    // program. Here, we are defining the calldata for the smart contract function we wish to
    // invoke.
    let mint_call_data = MintCallData {
        to: "0x1234567890123456789012345678901234567890", amount: 1000,
    };

    // The `execute` function of the Agents API is called here. It pauses the execution of the Cairo
    // VM, sends a request to the smart contract, and awaits a response which is then passed back
    // into the Cairo execution environment.
    let _response = AgentsApi::execute(
        ExecuteRequest {
            smart_account: "0x9876543210987654321098765432109876543210",
            calldata: Option::Some(mint_call_data),
            agent_id: "demo",
            entrypoint: "mint",
            contract: "0xE8eB593BDA8f8EAF644EaC73B4B88DB4c5dB25A5",
        }
    );
    // Use response as needed
"#,
        );
    }

    if project_config.oracle {
        content.push_str(
            r#"
    // ======== Custom Oracle Interaction ========
    // A custom oracle can be created to perform external calculations during the execution of a
    // Cairo program. In this case, the oracle calculates the square root of `n`.
    let oracle_response = SqrtOracle::oracle(OracleRequest { value: n }).result;

    // The result from the oracle is used to assert a condition in Cairo. This ensures the integrity
    // of the data returned by the oracle.
    assert!(oracle_response * oracle_response ==  n, "The oracle lied!");
    // Use oracle_response as needed
"#,
        );
    }

    content.push_str(
        r#"
    return n;
}
"#,
    );

    content
}

fn generate_proto_content(project_config: &ProjectConfig) -> String {
    let mut content = String::from(
        r#"
syntax = "proto3";

package oracle;

// Uncomment the line below to import definitions from Orion, if needed.
// import "orion.proto";
"#,
    );

    if project_config.agent_api {
        content.push_str(
            r#"
// ======== Agents API Service ======
// This section defines a Protocol Buffers service for interacting with a smart contract via
// Agents API. It includes data structures and an RPC method for executing smart contract
// functions such as minting tokens.

message MintCallData {
    string to = 1;    // Address of the recipient for minting tokens.
    uint64 amount = 2; // Amount of tokens to mint.
}

message AgentsAPIResponse {
    string transactionHash = 1; // Transaction hash returned by the blockchain.
    string status = 2;          // Status of the transaction, e.g., 'success' or 'error'.
}

message ExecuteRequest {
    string smartAccount = 1;    // Address of the smart account initiating the transaction.
    MintCallData calldata = 2;  // Calldata for the mint function.
    string agentId = 3;         // Identifier for the agent handling the request.
    string entrypoint = 4;      // Function to be called on the smart contract.
    string contract = 5;        // Address of the smart contract.
}

service AgentsAPI {
    // Executes a smart contract call using the provided request parameters.
    rpc Execute(ExecuteRequest) returns (AgentsAPIResponse) {}
}
"#,
        );
    }

    if project_config.oracle {
        content.push_str(
            r#"
// ======== Custom Oracle Service ======
// Defines a Protocol Buffers service for interacting with a custom oracle.
// This particular service handles requests to compute the square root of a given value.

message OracleRequest {
    int64 value = 1; // Value for which the square root will be calculated.
}

message OracleResponse {
    int64 result = 1; // Calculated result from the oracle.
}

service SqrtOracle {
    // Processes a request to calculate the square root of the specified value.
    rpc oracle(OracleRequest) returns (OracleResponse) {}
}
"#,
        );
    }

    content
}

fn generate_servers_json_content(project_config: &ProjectConfig) -> String {
    let mut servers = serde_json::Map::new();

    if project_config.agent_api {
        servers.insert(
            "execute".to_string(),
            json!({
                "server_url": "https://agents-api-6nn4ryaqca-ew.a.run.app/thirdweb/sessions",
                "polling": false
            }),
        );
    }

    if project_config.oracle {
        servers.insert(
            "oracle".to_string(),
            json!({
                "server_url": "http://127.0.0.1:3000",
                "polling": false
            }),
        );
    }

    serde_json::to_string_pretty(&servers).unwrap()
}

fn generate_inputs_schema_content() -> String {
    String::from(
        r#"
Input {
    n: i64
}
    "#,
    )
}
