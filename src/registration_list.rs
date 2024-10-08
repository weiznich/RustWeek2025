//! Render a list of all participants for a specific competition grouped by races
use crate::app_state::{self, AppState};
use crate::database::schema::{
    categories, competitions, participants, races, special_categories, starts,
};
use crate::database::shared_models::{
    Competition, Race, SpecialCategories, SpecialCategoryPerParticipant,
};
use crate::database::Id;
use crate::errors::{Error, Result};
use axum::extract::Path;
use axum::response::Html;
use axum::Router;
use diesel::prelude::*;
use diesel::sqlite::Sqlite;
use serde::Serialize;
use time::PrimitiveDateTime;

pub fn routes() -> Router<app_state::State> {
    Router::new().route(
        "/:event_id/registration_list.html",
        axum::routing::get(render_registration_list),
    )
}

/// Data for a specific participants
#[derive(Queryable, Selectable, Debug, serde::Serialize, Identifiable)]
#[diesel(table_name = participants)]
#[diesel(check_for_backend(Sqlite))]
pub struct ParticipantEntry {
    /// id of the participant
    #[serde(skip)]
    id: Id,
    /// first name of the participant
    first_name: String,
    /// last name of the participant
    last_name: String,
    /// club of the participant
    club: Option<String>,
    /// birth year of the participant
    birth_year: i32,
    /// start time for this participant
    #[diesel(select_expression = starts::time)]
    start_time: PrimitiveDateTime,
    /// category label for this participant
    #[diesel(select_expression = categories::label)]
    class: String,
    /// name of the race the participant participantes in
    #[serde(skip)]
    #[diesel(select_expression = races::name)]
    race_name: String,
}

#[derive(Debug, serde::Serialize)]
struct ParticipantEntryWithSpecialCategory {
    /// inner participant data
    #[serde(flatten)]
    participant: ParticipantEntry,
    /// a list of flags whether a participant is part of a special category or not
    /// the order of this list is expected to match the order of ParticipantsPerRace::special_categories
    special_categories: Vec<bool>,
}

#[derive(Debug, serde::Serialize)]
struct ParticipantsPerRace {
    /// Name of the race
    race_name: String,
    /// A list of participants for this race ordered by age
    participants: Vec<ParticipantEntryWithSpecialCategory>,
    /// A list of special categories for this race
    special_categories: Vec<SpecialCategories>,
}

/// Data used to render the participant list
///
/// See `templates/registration_list.html` for the relevant template
#[derive(Serialize)]
struct RegistrationListData {
    /// race specific participant data
    race_map: Vec<ParticipantsPerRace>,
    /// general information about the competition
    competition_info: Competition,
}

#[expect(
    clippy::type_complexity,
    reason = "The complex type is used as hint for participants"
)]
#[axum::debug_handler(state = app_state::State)]
async fn render_registration_list(
    state: AppState,
    Path(competition_id): Path<Id>,
) -> Result<Html<String>> {
    // for loading this data you need to deal with different kinds of relations
    // You want to combine joins and associations here to load the required data
    //
    // Steps:
    //
    // * Similar to `competition_overview.rs` get a connection from
    // the state first
    // * For loading these data we need to combine several tables in one query
    // * We don't want to load everything in one query, but in a fixed number
    //   of queries
    // * For the first iteration we can just ignore the special categories
    //   and return a vector of empty vectors there.
    //     + For `SpecialCategories` that vector needs to be as long as the
    //       `races` result
    //     + For `SpecialCategoryPerParticipant` that vector needs to be as long
    //       as the `participants` vector
    // * For all results you can use `ResultType::as_select()` to select
    //   the right columns form your query
    // * As input we get a `competition_id`, we can use this to load the
    //   `Option<Competition>`
    // * The `Race` struct not only contains data from the `Race` table
    //   but also `Category` data. Therefore we need to join multiple tables
    //   there: `races` (has a `competition_id`) -> `starts` -> `categories`
    //   (to get the actual category data)
    //       + Make sure to read the API docs of `QueryDsl::inner_join`
    //         to understand how diesel supports join chains like this
    //       + We want to order the data in such a we get a list of
    //         categories grouped by race starting from the shortest
    //         race to the longest race (Hint: Shorter races have usually
    //         younger participants)
    //       + Make sure to order, filter and group the data as required
    // * Participants relate to an competition through a chain of tables
    //  `participants` -> `categories` -> `starts` -> `races` ( -> `competitions`)
    //     + We need to join these tables in that order
    //     + Again we cant to order the result by category, racename, age, name
    //  * For `special_categories` we want to use the Associations API from diesel
    //     + Start with this if the other part work
    //     + To get the categories per participant we need to use both joins and
    //       the associations API
    //     + For loading the special categories itself we only need to use the
    //       associations API
    //     + Make sure to group the data using the `grouped_by` method after
    //       loading
    let (
        participant_list,
        competition_info,
        races,
        special_categories,
        special_categories_per_participant,
    ) = state
        .with_connection(move |conn| {
            let competition_info = competitions::table
                .find(competition_id)
                .select(Competition::as_select())
                .first(conn)
                .optional()?;
            let races = races::table
                .inner_join(starts::table.inner_join(categories::table))
                .order_by((categories::from_age, races::name))
                .filter(races::competition_id.eq(competition_id))
                .group_by(races::id)
                .select(Race::as_select())
                .load(conn)?;

            let special_categories = SpecialCategories::belonging_to(&races)
                .select(SpecialCategories::as_select())
                .load(conn)?;
            //let special_categories = special_categories.grouped_by(&races);
            let special_categories = vec![Vec::<SpecialCategories>::new(); races.len()];

            let participants = participants::table
                .inner_join(categories::table.inner_join(starts::table.inner_join(races::table)))
                .filter(races::competition_id.eq(competition_id))
                .order_by((
                    categories::from_age,
                    races::name,
                    participants::birth_year.desc(),
                    participants::first_name,
                    participants::last_name,
                ))
                .select(ParticipantEntry::as_select())
                .load(conn)?;

            let special_categories_per_participant =
                SpecialCategoryPerParticipant::belonging_to(&participants)
                    .inner_join(special_categories::table)
                    .select(SpecialCategoryPerParticipant::as_select())
                    .load(conn)?;

            // let special_categories_per_participant =
            //     special_categories_per_participant.grouped_by(&participants);
            let special_categories_per_participant =
                vec![Vec::<SpecialCategoryPerParticipant>::new(); participants.len()];

            Ok((
                participants,
                competition_info,
                races,
                special_categories,
                special_categories_per_participant,
            ))
        })
        .await?;
    let competition_info = competition_info
        .ok_or_else(|| Error::NotFound(format!("No competition for id {competition_id} found")))?;

    let mut participant_iter = participant_list
        .into_iter()
        .zip(special_categories_per_participant)
        .peekable();

    let race_map = races
        .into_iter()
        .zip(special_categories)
        .map(|(race, special_categories)| {
            let mut participants = Vec::new();
            while let Some((p, _special_categories_per_participant)) = participant_iter.peek() {
                if *p.race_name == race.name {
                    let (p, special_categories_per_participant) =
                        participant_iter.next().expect("We peeked");

                    let special_categories = special_categories
                        .iter()
                        .map(|cat| {
                            special_categories_per_participant
                                .iter()
                                .any(|c| c.special_category_id == cat.id)
                        })
                        .collect();
                    participants.push(ParticipantEntryWithSpecialCategory {
                        participant: p,
                        special_categories,
                    });
                } else {
                    break;
                }
            }
            ParticipantsPerRace {
                race_name: race.name,
                participants,
                special_categories,
            }
        })
        .collect::<Vec<_>>();

    state.render_template(
        "registration_list.html",
        RegistrationListData {
            race_map,
            competition_info,
        },
    )
}
