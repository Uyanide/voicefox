use std::sync::Arc;

use lx_core::model::song::SongInfo;
use lx_core::model::source::SourceId;
use lx_source::manager::SourceManager;

#[derive(Clone)]
pub struct SearchService {
    sources: Arc<SourceManager>,
}

impl SearchService {
    pub fn new(sources: Arc<SourceManager>) -> Self {
        Self { sources }
    }

    pub async fn search(
        &self,
        keyword: &str,
        page: u32,
        source: Option<SourceId>,
    ) -> Result<(Vec<SongInfo>, bool), String> {
        let result = self
            .sources
            .search_scoped(keyword, page, 50, source)
            .await
            .map_err(|e| e.to_string())?;
        Ok((result.items, result.has_more))
    }

    pub async fn parse_link(
        &self,
        keyword: &str,
    ) -> Result<(SourceId, lx_core::traits::source::ParsedLink), String> {
        self.sources.parse_link(keyword).await
    }
}
