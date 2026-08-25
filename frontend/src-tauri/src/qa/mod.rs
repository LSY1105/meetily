//! Meeting Q&A: ask questions against one meeting or the whole corpus,
//! with optional web search enrichment (Tavily).
//!
//! Answers must cite sources as `[n]`; the resolved source list is returned
//! alongside the answer so the UI can render clickable references.

pub mod commands;
pub mod context;
