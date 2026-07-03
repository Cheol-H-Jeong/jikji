#![forbid(unsafe_code)]

mod answer_pack;
mod artifacts;
mod brief;
mod cache_text;
mod discover;
mod discover_contract;
mod discover_query;
mod graph;
mod graph_artifacts;
mod index_rows;
mod indexer;
mod io;
mod map_query;
mod map_rescore;
mod scoring;
mod searcher;
mod sqlite_index;
mod status;
mod stopwords;
mod tokenizer;

pub use artifacts::{SearchArtifactStats, build_search_artifacts};
pub use brief::{BriefOptions, brief_payload, compact_brief_payload};
pub use discover::{DiscoverOptions, discover};
pub use graph::{explain_source, graph_query, graph_status};
pub use searcher::{SearchCandidate, SearchOptions, search};
pub use status::{IndexStatus, SearchIndexStatus, search_index_status};

pub const SEARCH_INDEX_SCHEMA_VERSION: u8 = 3;
