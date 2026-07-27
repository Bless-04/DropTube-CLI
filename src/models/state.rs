use crate::models::video::VideoFile;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState { //todo this should just have a Cli Args field
    pub movie_directory: PathBuf,
    pub port: u16,
    pub recurse: bool, 
    pub index_cache: Arc<RwLock<Vec<VideoFile>>>,
}

#[derive(Deserialize)]
pub struct HomeQuery {
    pub v: Option<String>,
}
