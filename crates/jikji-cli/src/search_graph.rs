use std::process::ExitCode;

use jikji_search::{explain_source, graph_query, graph_status};

use crate::args::{GraphArgs, GraphCommand};
use crate::output::print_json;

pub(crate) fn run_graph(args: GraphArgs) -> jikji_core::Result<ExitCode> {
    match args.command {
        GraphCommand::Status { json } => {
            let payload = graph_status(&args.root);
            if json {
                print_json(&payload)?;
            } else {
                println!("{payload}");
            }
        }
        GraphCommand::Query { query, top_k, json } => {
            let payload = serde_json::json!({
                "root": args.root.display().to_string(),
                "query": query,
                "candidates": graph_query(&args.root, &query, top_k)?,
            });
            if json {
                print_json(&payload)?;
            } else {
                println!("{payload}");
            }
        }
        GraphCommand::Explain { source_path, json } => {
            let payload = explain_source(&args.root, &source_path);
            if json {
                print_json(&payload)?;
            } else {
                println!("{payload}");
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}
