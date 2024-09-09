use crate::templates::get_template_engine;
use crate::{fsx, ProjectConfig};
use anyhow::Result;
use camino::Utf8PathBuf;
use once_cell::sync::Lazy;
use serde_json::json;

const SERVER_MANIFEST_PATH: Lazy<Utf8PathBuf> =
    Lazy::new(|| ["python", "requirements.txt"].iter().collect());
const SERVER_SOURCE_PATH: Lazy<Utf8PathBuf> =
    Lazy::new(|| ["python/src", "main.py"].iter().collect());
const INIT_SOURCE_PATH: Lazy<Utf8PathBuf> =
    Lazy::new(|| ["python/src", "__init__.py"].iter().collect());

pub(crate) fn mk_python(
    canonical_path: &Utf8PathBuf,
    project_config: &ProjectConfig,
) -> Result<()> {
    // Get the templates registry
    let registry = get_template_engine();

    // Create the `requirements.txt` file.
    let filename = canonical_path.join(SERVER_MANIFEST_PATH.as_path());
    if !filename.exists() {
        fsx::create_dir_all(filename.parent().unwrap())?;

        fsx::write(filename, registry.render("requirements", &json!({}))?)?;
    }

    // Create the `__init__.py` file.
    let filename = canonical_path.join(INIT_SOURCE_PATH.as_path());
    if !filename.exists() {
        fsx::create_dir_all(filename.parent().unwrap())?;

        fsx::write(filename, "")?;
    }

    // Create the `main.py` file.
    let filename = canonical_path.join(SERVER_SOURCE_PATH.as_path());
    if !filename.exists() {
        fsx::create_dir_all(filename.parent().unwrap())?;

        let main_content = generate_main_py_content(project_config);
        fsx::write(filename, main_content)?;
    }

    Ok(())
}

fn generate_main_py_content(project_config: &ProjectConfig) -> String {
    let mut content = String::from(
        r#"
import math
from fastapi import FastAPI, Request, HTTPException
from pydantic import BaseModel
import json

app = FastAPI()

@app.get("/healthcheck")
def read_root():
    """
    Health check endpoint to ensure the API is up and running.
    Returns a simple JSON response indicating the API status.
    """
    return {"status": "OK"}
"#,
    );

    if project_config.preprocess {
        content.push_str(
            r#"
# ========== Preprocessing ==========
# This endpoint handles preprocessing of data before executing a Cairo program.
# It formats and prepares the input data, making it ready for the Cairo main function.
@app.post("/preprocess")
async def preprocess(request: Request):
    """
    Receives JSON data, processes it, and returns the modified data
    as arguments for a Cairo main function.
    """
    data = await request.json()
    # Insert custom preprocessing logic here
    processed_data = {"n": data["n"]}
    return {"args": json.dumps(processed_data)}
"#,
        );
    }

    if project_config.postprocess {
        content.push_str(
            r#"
# ========== Postprocessing ==========
# This endpoint handles postprocessing of data after a Cairo program execution.
# It allows further manipulation or interpretation of the Cairo output.
@app.post("/postprocess")
async def postprocess(request: Request):
    """
    Receives JSON data as the output of a Cairo main function, processes it,
    and returns the modified result.
    """
    data = await request.json()
    # Insert custom postprocessing logic here
    processed_data = {"processed": data}
    return processed_data
"#,
        );
    }

    if project_config.oracle {
        content.push_str(
            r#"
# ========== Custom Oracle ==========
# Defines an endpoint for a custom oracle that provides external data or computations
# required by a Cairo program during its execution.
@app.post("/oracle")
async def oracle(request: Request):
    """
    Custom oracle logic that processes incoming data and returns a result.
    This endpoint acts as a middleman for external computations or data retrievals
    required by the Cairo program.
    """
    data = await request.json()
    # Insert custom oracle logic here
    sqrt = int(math.sqrt(data["value"]))
    result = {"result": sqrt}
    return result
"#,
        );
    }

    content.push_str(
        r#"
if __name__ == "__main__":
    import uvicorn

    # Configures and runs the API server on host 0.0.0.0 at port 3000 with auto-reload enabled.
    uvicorn.run("main:app", host="0.0.0.0", port=3000, reload=True)
"#,
    );

    content
}
