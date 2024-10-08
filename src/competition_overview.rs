//! A list of the various competitions in the database

use crate::app_state::AppState;
use crate::database::shared_models::Competition;
use crate::errors::Result;
use axum::response::Html;
use serde::Serialize;

#[derive(Serialize)]
struct CompetitionList {
    competitions: Vec<Competition>,
}

#[axum::debug_handler(state = crate::app_state::State)]
pub async fn render(state: AppState) -> Result<Html<String>> {
    let competitions: Vec<Competition> = state
        .with_connection(move |_conn| {
            // start here implementing loading competation data from the database
            //
            // Steps to perform:
            //
            // * Rename the `_conn` variable to `conn`
            // * Import the `competitions` schema module
            // * Select relevant columns via `Competition::as_select()`
            // * Load all rows from the competition table
            todo!()
        })
        .await?;

    state.render_template("competition_list.html", CompetitionList { competitions })
}
