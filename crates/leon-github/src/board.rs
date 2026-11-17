//! An issue's fields on its GitHub Project boards: the "Status" and
//! "Priority" single-selects, and its iterations (sprints).

use leon_domain::{BoardOption, Iteration};
use serde_json::Value;

use crate::parse::{nodes, text};

/// The single-select field that gives the board's column.
const STATUS: &str = "Status";
/// The single-select field that gives the issue's priority.
const PRIORITY: &str = "Priority";

/// What an issue's boards say; empty when the query did not ask for them.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Board {
    pub(crate) status: Option<BoardOption>,
    pub(crate) priority: Option<BoardOption>,
    pub(crate) iterations: Vec<Iteration>,
}

/// The board fields of an issue node. On several boards, the first value
/// found per field wins; field names match whatever their case.
pub(crate) fn board(issue: &Value) -> Board {
    let mut board = Board::default();
    let values = nodes(&issue["projectItems"]).flat_map(|item| nodes(&item["fieldValues"]));
    for value in values {
        match value["__typename"].as_str() {
            Some("ProjectV2ItemFieldSingleSelectValue") => board.single_select(value),
            // A single-select issue field of the organization, shown on the
            // board (how GitHub's own "Priority" field comes, 2026-11-18).
            Some("ProjectV2ItemIssueFieldValue") => {
                board.single_select(&value["issueFieldValue"]);
            }
            Some("ProjectV2ItemFieldIterationValue") => board.iteration(value),
            _ => {}
        }
    }
    board
}

impl Board {
    /// Keeps a "Status" or "Priority" value unless one was kept already.
    fn single_select(&mut self, value: &Value) {
        let Some(field) = value["field"]["name"].as_str() else {
            return;
        };
        let slot = if field.eq_ignore_ascii_case(STATUS) {
            &mut self.status
        } else if field.eq_ignore_ascii_case(PRIORITY) {
            &mut self.priority
        } else {
            return;
        };
        if slot.is_none() {
            *slot = option(value);
        }
    }

    /// Keeps an iteration unless its field has one already.
    fn iteration(&mut self, value: &Value) {
        let Some(iteration) = iteration(value) else {
            return;
        };
        if !self.iterations.iter().any(|i| i.field == iteration.field) {
            self.iterations.push(iteration);
        }
    }
}

/// A single-select value; its order is the option's place in the field.
fn option(value: &Value) -> Option<BoardOption> {
    let options = value["field"]["options"].as_array();
    let order = value["optionId"]
        .as_str()
        .and_then(|id| options?.iter().position(|o| o["id"] == id))
        .unwrap_or(usize::MAX);
    Some(BoardOption {
        name: text(&value["name"])?,
        order,
        color: text(&value["color"]),
    })
}

fn iteration(value: &Value) -> Option<Iteration> {
    Some(Iteration {
        field: text(&value["field"]["name"])?,
        title: text(&value["title"])?,
        start: text(&value["startDate"])?,
    })
}

#[cfg(test)]
mod tests {
    use leon_domain::{BoardOption, Iteration};
    use serde_json::{Value, json};

    use super::{Board, board};

    fn select(field: &str, name: &str, option: &str) -> Value {
        json!({ "__typename": "ProjectV2ItemFieldSingleSelectValue", "name": name,
            "color": "BLUE", "optionId": option,
            "field": { "name": field, "options": [{ "id": "a" }, { "id": "b" }] } })
    }

    fn issue_field(field: &str, name: &str, option: &str) -> Value {
        let mut value = select(field, name, option);
        value["__typename"] = "IssueFieldSingleSelectValue".into();
        json!({ "__typename": "ProjectV2ItemIssueFieldValue", "issueFieldValue": value })
    }

    fn sprint(field: &str, title: &str) -> Value {
        json!({ "__typename": "ProjectV2ItemFieldIterationValue", "title": title,
            "startDate": "2026-11-23", "field": { "name": field } })
    }

    /// One issue on boards, each board a list of field values.
    fn on_boards(boards: &[&[Value]]) -> Value {
        let items: Vec<Value> = boards
            .iter()
            .map(|values| json!({ "fieldValues": { "nodes": values } }))
            .collect();
        json!({ "projectItems": { "nodes": items } })
    }

    fn option(name: &str, order: usize) -> BoardOption {
        BoardOption {
            name: name.into(),
            order,
            color: Some("BLUE".into()),
        }
    }

    fn iteration(field: &str, title: &str) -> Iteration {
        Iteration {
            field: field.into(),
            title: title.into(),
            start: "2026-11-23".into(),
        }
    }

    #[test]
    fn board_fields() {
        let other = json!({ "__typename": "ProjectV2ItemFieldTextValue" });
        let cases = [
            (json!({}), Board::default()),
            (json!({ "projectItems": null }), Board::default()),
            (
                on_boards(&[&[other.clone(), select("Status", "Doing", "b")]]),
                Board {
                    status: Some(option("Doing", 1)),
                    ..Board::default()
                },
            ),
            (
                on_boards(&[&[select("status", "Todo", "a"), select("PRIORITY", "P1", "b")]]),
                Board {
                    status: Some(option("Todo", 0)),
                    priority: Some(option("P1", 1)),
                    ..Board::default()
                },
            ),
            (
                on_boards(&[&[issue_field("Priority", "P0", "a")]]),
                Board {
                    priority: Some(option("P0", 0)),
                    ..Board::default()
                },
            ),
            (
                on_boards(&[
                    &[select("Size", "XL", "a"), sprint("Sprint", "Sprint 2")],
                    &[select("Status", "Done", "b"), sprint("Sprint", "Sprint 9")],
                    &[select("Status", "Todo", "a"), sprint("Quarter", "Q4")],
                ]),
                Board {
                    status: Some(option("Done", 1)),
                    iterations: vec![iteration("Sprint", "Sprint 2"), iteration("Quarter", "Q4")],
                    ..Board::default()
                },
            ),
            (
                on_boards(&[&[
                    json!({ "__typename": "ProjectV2ItemIssueFieldValue", "issueFieldValue": {} }),
                    json!({ "__typename": "ProjectV2ItemFieldSingleSelectValue", "name": null }),
                    json!({ "__typename": "ProjectV2ItemFieldIterationValue", "title": "No field" }),
                    select("Status", "Unlisted", "z"),
                ]]),
                Board {
                    status: Some(option("Unlisted", usize::MAX)),
                    ..Board::default()
                },
            ),
        ];
        for (issue, expected) in cases {
            assert_eq!(board(&issue), expected, "{issue}");
        }
    }

    #[test]
    fn missing_option_id_or_options_sort_last() {
        let mut value = select("Status", "Todo", "a");
        value["optionId"] = Value::Null;
        assert_eq!(
            board(&on_boards(&[&[value]])).status,
            Some(option("Todo", usize::MAX))
        );
        let mut value = select("Status", "Todo", "a");
        value["field"]["options"] = Value::Null;
        assert_eq!(
            board(&on_boards(&[&[value]])).status,
            Some(option("Todo", usize::MAX))
        );
    }
}
