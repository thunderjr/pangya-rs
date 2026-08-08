//! Explicit LoginService application state machine.

use pangya_domain::SetupState;
use thiserror::Error;

/// LoginService connection state independent of transport and storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoginState {
    /// No authenticated identity exists.
    AwaitLogin,
    /// Authenticated account must check/set a nickname.
    AwaitNicknameCheckOrSet,
    /// Authenticated account must choose an allowed starter character.
    AwaitCharacterSelect,
    /// A handover must be generated and persisted.
    IssueHandover,
    /// A configured GameService selection is expected.
    AwaitServerSelect,
    /// Successful terminal application state.
    Complete,
    /// Disconnected or rejected terminal state.
    Closed,
}

/// Validated application event; packet payload validation happens before this boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoginEvent {
    /// Credentials authenticated with the persisted setup state.
    Authenticated(SetupState),
    /// A credential attempt failed with a friendly outcome.
    AuthenticationRejected,
    /// A nickname availability check completed without selecting it.
    NicknameChecked {
        /// Whether this check avoided consuming a friendly-failure retry.
        available: bool,
    },
    /// A nickname set lost a duplicate race and consumed a friendly-failure retry.
    NicknameRejected,
    /// Nickname was selected; `true` means starter selection remains necessary.
    NicknameSet {
        /// Whether first-character setup remains after nickname persistence.
        needs_character: bool,
    },
    /// An allowlisted starter character was selected and persisted.
    CharacterSelected,
    /// The 60-second handover was persisted.
    HandoverIssued,
    /// The configured server ID was selected.
    ServerSelected,
    /// Peer or server initiated disconnect.
    Disconnect,
}

/// Rejected state-machine transition.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("event is invalid in the current login state")]
pub struct TransitionError;

/// Stateful transition driver with bounded friendly retries.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LoginStateMachine {
    state: LoginState,
    retries: u8,
    max_retries: u8,
}

impl LoginStateMachine {
    /// Creates a machine in [`LoginState::AwaitLogin`].
    ///
    /// # Errors
    /// Returns [`TransitionError`] when the retry bound is zero.
    pub const fn new(max_retries: u8) -> Result<Self, TransitionError> {
        if max_retries == 0 {
            return Err(TransitionError);
        }
        Ok(Self {
            state: LoginState::AwaitLogin,
            retries: 0,
            max_retries,
        })
    }

    /// Returns the current state.
    #[must_use]
    pub const fn state(&self) -> LoginState {
        self.state
    }

    /// Applies one validated event. Invalid events never mutate the machine.
    ///
    /// # Errors
    /// Returns [`TransitionError`] for an event not accepted in the current state.
    pub fn apply(&mut self, event: LoginEvent) -> Result<LoginState, TransitionError> {
        if event == LoginEvent::Disconnect {
            self.state = LoginState::Closed;
            return Ok(self.state);
        }
        let next = match (self.state, event) {
            (LoginState::AwaitLogin, LoginEvent::Authenticated(SetupState::NeedsNickname)) => {
                self.retries = 0;
                LoginState::AwaitNicknameCheckOrSet
            }
            (LoginState::AwaitLogin, LoginEvent::Authenticated(SetupState::NeedsStarter)) => {
                self.retries = 0;
                LoginState::AwaitCharacterSelect
            }
            (LoginState::AwaitLogin, LoginEvent::Authenticated(SetupState::Complete)) => {
                self.retries = 0;
                LoginState::IssueHandover
            }
            (LoginState::AwaitLogin, LoginEvent::AuthenticationRejected) => {
                self.retry_or_close(LoginState::AwaitLogin)
            }
            (
                LoginState::AwaitNicknameCheckOrSet,
                LoginEvent::NicknameChecked { available: true },
            ) => LoginState::AwaitNicknameCheckOrSet,
            (
                LoginState::AwaitNicknameCheckOrSet,
                LoginEvent::NicknameChecked { available: false } | LoginEvent::NicknameRejected,
            ) => self.retry_or_close(LoginState::AwaitNicknameCheckOrSet),
            (LoginState::AwaitNicknameCheckOrSet, LoginEvent::NicknameSet { needs_character }) => {
                self.retries = 0;
                if needs_character {
                    LoginState::AwaitCharacterSelect
                } else {
                    LoginState::IssueHandover
                }
            }
            (LoginState::AwaitCharacterSelect, LoginEvent::CharacterSelected) => {
                self.retries = 0;
                LoginState::IssueHandover
            }
            (LoginState::IssueHandover, LoginEvent::HandoverIssued) => {
                LoginState::AwaitServerSelect
            }
            (LoginState::AwaitServerSelect, LoginEvent::ServerSelected) => LoginState::Complete,
            _ => return Err(TransitionError),
        };
        self.state = next;
        Ok(next)
    }

    fn retry_or_close(&mut self, retry_state: LoginState) -> LoginState {
        self.retries = self.retries.saturating_add(1);
        if self.retries >= self.max_retries {
            LoginState::Closed
        } else {
            retry_state
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_driven_valid_transitions() {
        let cases = [
            (
                SetupState::NeedsNickname,
                LoginState::AwaitNicknameCheckOrSet,
            ),
            (SetupState::NeedsStarter, LoginState::AwaitCharacterSelect),
            (SetupState::Complete, LoginState::IssueHandover),
        ];
        for (setup, expected) in cases {
            let mut machine = LoginStateMachine::new(3).expect("machine");
            assert_eq!(
                machine.apply(LoginEvent::Authenticated(setup)),
                Ok(expected)
            );
        }
    }

    #[test]
    fn rejected_events_do_not_mutate_any_state() {
        let states = [
            LoginState::AwaitLogin,
            LoginState::AwaitNicknameCheckOrSet,
            LoginState::AwaitCharacterSelect,
            LoginState::IssueHandover,
            LoginState::AwaitServerSelect,
            LoginState::Complete,
            LoginState::Closed,
        ];
        for state in states {
            let mut machine = LoginStateMachine {
                state,
                retries: 0,
                max_retries: 3,
            };
            let before = machine.clone();
            let rejected = if state == LoginState::AwaitLogin {
                LoginEvent::ServerSelected
            } else {
                LoginEvent::AuthenticationRejected
            };
            assert_eq!(machine.apply(rejected), Err(TransitionError));
            assert_eq!(machine, before);
        }
    }

    #[test]
    fn disconnect_is_valid_from_every_state() {
        for state in [
            LoginState::AwaitLogin,
            LoginState::AwaitNicknameCheckOrSet,
            LoginState::AwaitCharacterSelect,
            LoginState::IssueHandover,
            LoginState::AwaitServerSelect,
            LoginState::Complete,
            LoginState::Closed,
        ] {
            let mut machine = LoginStateMachine {
                state,
                retries: 0,
                max_retries: 3,
            };
            assert_eq!(
                machine.apply(LoginEvent::Disconnect),
                Ok(LoginState::Closed)
            );
        }
    }

    #[test]
    fn retries_are_bounded() {
        let mut machine = LoginStateMachine::new(2).expect("machine");
        assert_eq!(
            machine.apply(LoginEvent::AuthenticationRejected),
            Ok(LoginState::AwaitLogin)
        );
        assert_eq!(
            machine.apply(LoginEvent::AuthenticationRejected),
            Ok(LoginState::Closed)
        );
    }

    #[test]
    fn available_nickname_checks_do_not_reset_cumulative_failures() {
        let mut machine = LoginStateMachine::new(3).expect("machine");
        assert_eq!(
            machine.apply(LoginEvent::Authenticated(SetupState::NeedsNickname)),
            Ok(LoginState::AwaitNicknameCheckOrSet)
        );
        for available in [false, true, false, true] {
            assert_eq!(
                machine.apply(LoginEvent::NicknameChecked { available }),
                Ok(LoginState::AwaitNicknameCheckOrSet)
            );
        }
        assert_eq!(
            machine.apply(LoginEvent::NicknameChecked { available: false }),
            Ok(LoginState::Closed)
        );
    }
}
