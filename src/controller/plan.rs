//! Validierung des vom Brain gelieferten Aktionsplans.

pub fn validate_action_plan(actions: &[crate::protocol::Action]) -> Result<(), String> {
    if actions.is_empty() {
        return Err("Aktionsplan ist leer".to_string());
    }

    for (idx, action) in actions.iter().enumerate() {
        if action.id.trim().is_empty() {
            return Err(format!("Action an Position {} hat keine ID", idx));
        }

        match action.action_type {
            crate::protocol::ActionType::Shell => {
                if action.command.trim().is_empty() {
                    return Err(format!(
                        "Shell-Action '{}' hat keinen Befehl (command)",
                        action.id
                    ));
                }
            }
            crate::protocol::ActionType::Message => {
                if action.text.trim().is_empty() {
                    return Err(format!(
                        "Message-Action '{}' hat keinen Text (text)",
                        action.id
                    ));
                }
            }
            crate::protocol::ActionType::Finish => {}
            crate::protocol::ActionType::Edit => {
                if action.path.trim().is_empty() {
                    return Err(format!(
                        "Edit-Action '{}' hat keinen Pfad (path)",
                        action.id
                    ));
                }
            }
            crate::protocol::ActionType::Write => {
                if action.path.trim().is_empty() {
                    return Err(format!(
                        "Write-Action '{}' hat keinen Pfad (path)",
                        action.id
                    ));
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod validate_action_plan_tests {
    use super::*;
    use crate::protocol::{Action, ActionType};

    #[test]
    fn test_empty_plan_rejected() {
        let plan: Vec<Action> = vec![];
        assert!(validate_action_plan(&plan).is_err());
    }

    #[test]
    fn test_valid_shell_and_file_actions_accepted() {
        let plan = vec![
            Action {
                id: "step-1".to_string(),
                action_type: ActionType::Shell,
                command: "Get-Location".to_string(),
                text: "".to_string(),
                timeout_seconds: 30.0,
                path: "".to_string(),
                old_string: "".to_string(),
                new_string: "".to_string(),
                content: "".to_string(),
            },
            Action {
                id: "write-1".to_string(),
                action_type: ActionType::Write,
                command: "".to_string(),
                text: "".to_string(),
                timeout_seconds: 30.0,
                path: "src/test.txt".to_string(),
                old_string: "".to_string(),
                new_string: "".to_string(),
                content: "hello".to_string(),
            },
        ];
        assert!(validate_action_plan(&plan).is_ok());
    }

    #[test]
    fn test_missing_required_fields_rejected() {
        let plan_missing_cmd = vec![Action {
            id: "step-1".to_string(),
            action_type: ActionType::Shell,
            command: "   ".to_string(),
            text: "".to_string(),
            timeout_seconds: 30.0,
            path: "".to_string(),
            old_string: "".to_string(),
            new_string: "".to_string(),
            content: "".to_string(),
        }];
        assert!(validate_action_plan(&plan_missing_cmd).is_err());

        let plan_missing_path = vec![Action {
            id: "step-2".to_string(),
            action_type: ActionType::Write,
            command: "".to_string(),
            text: "".to_string(),
            timeout_seconds: 30.0,
            path: "".to_string(),
            old_string: "".to_string(),
            new_string: "".to_string(),
            content: "hello".to_string(),
        }];
        assert!(validate_action_plan(&plan_missing_path).is_err());
    }
}
