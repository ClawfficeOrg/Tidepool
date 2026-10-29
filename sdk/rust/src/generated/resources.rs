// Generated from docs/api/openapi-v1.json. Do not edit by hand.
// Regenerate: node scripts/generate.mjs (from sdk/rust).

//! Generated namespace wrappers for both clients.
//!
//! The blocking module is compiled by the `blocking` feature and the async
//! module by the `async` feature; the request types in `endpoints` are shared.

#[cfg(feature = "blocking")]
pub mod blocking_api {
    //! Typed blocking calls: one function per contract route.

    /// `activity` resources.
    pub mod activity {
        /// List one page of receipts, runs, steps and executions
        pub fn list(
            client: &crate::blocking::Client,
            query: crate::endpoints::GetV1ActivityQuery,
        ) -> crate::Result<crate::types::ActivityPage> {
            client.call(&crate::endpoints::GetV1Activity::new().with_query(query))
        }

        /// Fetch one run with the arrival record that started it
        pub fn run(
            client: &crate::blocking::Client,
            run: &str,
        ) -> crate::Result<crate::types::ActivityRunDetail> {
            client.call(&crate::endpoints::GetV1ActivityRunsRun::new(run))
        }
    }

    /// `apiKeys` resources.
    pub mod api_keys {
        /// Create an API key; the full secret is shown once
        pub fn create(
            client: &crate::blocking::Client,
            body: crate::types::CreateApiKeyRequest,
        ) -> crate::Result<crate::types::CreateApiKeyResponse> {
            client.call(&crate::endpoints::PostV1ApiKeys::new(body))
        }

        /// List organization API keys (owner only; prefix only)
        pub fn list(
            client: &crate::blocking::Client,
        ) -> crate::Result<crate::types::ApiKeysResponse> {
            client.call(&crate::endpoints::GetV1ApiKeys::new())
        }

        /// Revoke an API key (owner only; effective immediately)
        pub fn remove(client: &crate::blocking::Client, id: &str) -> crate::Result<()> {
            client.call(&crate::endpoints::DeleteV1ApiKeysId::new(id))
        }

        /// Introspect an API key (requires projects:read)
        pub fn whoami(client: &crate::blocking::Client) -> crate::Result<crate::types::ApiKeyView> {
            client.call(&crate::endpoints::GetV1ApiKeysWhoami::new())
        }
    }

    /// `chatrooms` resources.
    pub mod chatrooms {
        /// List rooms visible to the operator
        pub fn list(client: &crate::blocking::Client) -> crate::Result<crate::types::RoomViewList> {
            client.call(&crate::endpoints::GetV1Chatrooms::new())
        }

        /// List a room's members
        pub fn members(
            client: &crate::blocking::Client,
            room: &str,
        ) -> crate::Result<crate::types::RoomMemberViewList> {
            client.call(&crate::endpoints::GetV1ChatroomsRoomMembers::new(room))
        }

        /// Read room history after a sequence cursor
        pub fn messages(
            client: &crate::blocking::Client,
            room: &str,
            query: crate::endpoints::GetV1ChatroomsRoomMessagesQuery,
        ) -> crate::Result<crate::types::HistoryResponse> {
            client.call(&crate::endpoints::GetV1ChatroomsRoomMessages::new(room).with_query(query))
        }

        /// Post a message to a room
        pub fn send(
            client: &crate::blocking::Client,
            room: &str,
            body: crate::types::SendMessageBody,
        ) -> crate::Result<crate::types::MessageView> {
            client.call(&crate::endpoints::PostV1ChatroomsRoomMessages::new(
                room, body,
            ))
        }

        /// Upgrade to the room's live message stream
        pub fn stream(
            client: &crate::blocking::Client,
            room: &str,
            query: crate::endpoints::GetV1ChatroomsRoomStreamQuery,
        ) -> crate::Result<crate::stream::ChatStream> {
            client.stream(&crate::endpoints::GetV1ChatroomsRoomStream::new(room).with_query(query))
        }
    }

    /// `connections` resources.
    pub mod connections {
        /// Create a provider connection from a credential reference
        pub fn create(
            client: &crate::blocking::Client,
            body: crate::types::CreateConnectionRequest,
        ) -> crate::Result<crate::types::ConnectionView> {
            client.call(&crate::endpoints::PostV1Connections::new(body))
        }

        /// Fetch a connection by id or env:<slug>
        pub fn get(
            client: &crate::blocking::Client,
            id: &str,
        ) -> crate::Result<crate::types::ConnectionView> {
            client.call(&crate::endpoints::GetV1ConnectionsId::new(id))
        }

        /// List effective provider connections (environment overlay applied)
        pub fn list(
            client: &crate::blocking::Client,
        ) -> crate::Result<crate::types::ConnectionsResponse> {
            client.call(&crate::endpoints::GetV1Connections::new())
        }

        /// Delete a database connection
        pub fn remove(client: &crate::blocking::Client, id: &str) -> crate::Result<()> {
            client.call(&crate::endpoints::DeleteV1ConnectionsId::new(id))
        }

        /// Rename, enable or disable a database connection
        pub fn update(
            client: &crate::blocking::Client,
            id: &str,
            body: crate::types::UpdateConnectionRequest,
        ) -> crate::Result<crate::types::ConnectionView> {
            client.call(&crate::endpoints::PatchV1ConnectionsId::new(id, body))
        }
    }

    /// `game` resources.
    pub mod game {
        /// `auth` resources.
        pub mod auth {
            /// Verify credentials and issue a signed JWT
            pub fn login(
                client: &crate::blocking::Client,
                body: crate::types::JsonValue,
            ) -> crate::Result<crate::types::JsonObject> {
                client.call(&crate::endpoints::PostV1AuthLogin::new(body))
            }

            /// Register a new player and return the public profile
            pub fn register(
                client: &crate::blocking::Client,
                body: crate::types::JsonValue,
            ) -> crate::Result<crate::types::JsonObject> {
                client.call(&crate::endpoints::PostV1AuthRegister::new(body))
            }
        }

        /// `events` resources.
        pub mod events {
            /// Ingest a batch of analytics events (fire-and-forget)
            pub fn create(
                client: &crate::blocking::Client,
                body: crate::types::JsonValue,
            ) -> crate::Result<crate::types::JsonObject> {
                client.call(&crate::endpoints::PostV1Events::new(body))
            }
        }

        /// `feedback` resources.
        pub mod feedback {
            /// Submit player feedback
            pub fn create(
                client: &crate::blocking::Client,
                body: crate::types::JsonValue,
            ) -> crate::Result<crate::types::JsonObject> {
                client.call(&crate::endpoints::PostV1Feedback::new(body))
            }
        }

        /// `games` resources.
        pub mod games {
            /// Fetch a game's settings
            pub fn settings(
                client: &crate::blocking::Client,
                game_id: &str,
            ) -> crate::Result<crate::types::JsonObject> {
                client.call(&crate::endpoints::GetV1GamesGameIdSettings::new(game_id))
            }

            /// Set a game's settings (upsert)
            pub fn update_settings(
                client: &crate::blocking::Client,
                game_id: &str,
                body: crate::types::JsonValue,
            ) -> crate::Result<crate::types::JsonObject> {
                client.call(&crate::endpoints::PutV1GamesGameIdSettings::new(
                    game_id, body,
                ))
            }
        }

        /// `leaderboards` resources.
        pub mod leaderboards {
            /// Fetch one ranked page of a leaderboard
            pub fn get(
                client: &crate::blocking::Client,
                game_id: &str,
                query: crate::endpoints::GetV1LeaderboardsGameIdQuery,
            ) -> crate::Result<crate::types::JsonObject> {
                client.call(
                    &crate::endpoints::GetV1LeaderboardsGameId::new(game_id).with_query(query),
                )
            }

            /// Update the authenticated player's own leaderboard entry
            pub fn put_entry(
                client: &crate::blocking::Client,
                game_id: &str,
                player_id: &str,
                body: crate::types::JsonValue,
            ) -> crate::Result<crate::types::JsonObject> {
                client.call(
                    &crate::endpoints::PutV1LeaderboardsGameIdEntriesPlayerId::new(
                        game_id, player_id, body,
                    ),
                )
            }

            /// Submit a leaderboard entry for the authenticated player
            pub fn submit_entry(
                client: &crate::blocking::Client,
                game_id: &str,
                body: crate::types::JsonValue,
            ) -> crate::Result<crate::types::JsonObject> {
                client.call(&crate::endpoints::PostV1LeaderboardsGameIdEntries::new(
                    game_id, body,
                ))
            }
        }

        /// `players` resources.
        pub mod players {
            /// Create a subaccount under the authenticated player
            pub fn create_subaccount(
                client: &crate::blocking::Client,
                body: crate::types::JsonValue,
            ) -> crate::Result<crate::types::JsonObject> {
                client.call(&crate::endpoints::PostV1PlayersSubaccount::new(body))
            }

            /// Fetch a player's public profile
            pub fn get(
                client: &crate::blocking::Client,
                id: &str,
            ) -> crate::Result<crate::types::JsonObject> {
                client.call(&crate::endpoints::GetV1PlayersId::new(id))
            }

            /// Soft-delete the authenticated player's own account
            pub fn remove(client: &crate::blocking::Client, id: &str) -> crate::Result<()> {
                client.call(&crate::endpoints::DeleteV1PlayersId::new(id))
            }

            /// List the authenticated player's subaccounts
            pub fn subaccounts(
                client: &crate::blocking::Client,
                id: &str,
            ) -> crate::Result<crate::types::JsonObject> {
                client.call(&crate::endpoints::GetV1PlayersIdSubaccounts::new(id))
            }

            /// Update the authenticated player's own profile
            pub fn update(
                client: &crate::blocking::Client,
                id: &str,
                body: crate::types::JsonValue,
            ) -> crate::Result<crate::types::JsonObject> {
                client.call(&crate::endpoints::PutV1PlayersId::new(id, body))
            }
        }

        /// `saves` resources.
        pub mod saves {
            /// Create a game save
            pub fn create(
                client: &crate::blocking::Client,
                body: crate::types::JsonValue,
            ) -> crate::Result<crate::types::JsonObject> {
                client.call(&crate::endpoints::PostV1Saves::new(body))
            }

            /// Fetch a single game save
            pub fn get(
                client: &crate::blocking::Client,
                player_id: &str,
                save_id: &str,
            ) -> crate::Result<crate::types::JsonObject> {
                client.call(&crate::endpoints::GetV1SavesPlayerIdSaveId::new(
                    player_id, save_id,
                ))
            }

            /// List the authenticated player's saves, newest first
            pub fn list(
                client: &crate::blocking::Client,
                player_id: &str,
            ) -> crate::Result<crate::types::JsonObject> {
                client.call(&crate::endpoints::GetV1SavesPlayerId::new(player_id))
            }

            /// Delete a single game save
            pub fn remove(
                client: &crate::blocking::Client,
                player_id: &str,
                save_id: &str,
            ) -> crate::Result<()> {
                client.call(&crate::endpoints::DeleteV1SavesPlayerIdSaveId::new(
                    player_id, save_id,
                ))
            }
        }

        /// `socketTickets` resources.
        pub mod socket_tickets {
            /// Issue a one-shot WebSocket connection ticket
            pub fn create(
                client: &crate::blocking::Client,
                body: crate::types::JsonValue,
            ) -> crate::Result<crate::types::JsonObject> {
                client.call(&crate::endpoints::PostV1SocketTickets::new(body))
            }
        }
    }

    /// `hosts` resources.
    pub mod hosts {
        /// Read the host's last capability report and offered providers
        pub fn capabilities(
            client: &crate::blocking::Client,
            id: &str,
        ) -> crate::Result<crate::types::HostCapabilityReport> {
            client.call(&crate::endpoints::GetV1HostsIdCapabilities::new(id))
        }

        /// Register a host; the slug is derived from the hostname
        pub fn create(
            client: &crate::blocking::Client,
            body: crate::types::CreateHostRequest,
        ) -> crate::Result<crate::types::HostView> {
            client.call(&crate::endpoints::PostV1Hosts::new(body))
        }

        /// Create or replay a create-agent intent with a stable execution id
        pub fn create_execution(
            client: &crate::blocking::Client,
            id: &str,
            body: crate::types::CreateExecutionRequest,
        ) -> crate::Result<crate::types::CreateExecutionResponse> {
            client.call(&crate::endpoints::PostV1HostsIdExecutions::new(id, body))
        }

        /// Exchange an enrollment token for a host relationship credential
        pub fn enroll(
            client: &crate::blocking::Client,
            body: crate::types::EnrollHostRequest,
        ) -> crate::Result<crate::types::HostView> {
            client.call(&crate::endpoints::PostV1HostsEnroll::new(body))
        }

        /// Fetch a live host by immutable id
        pub fn get(
            client: &crate::blocking::Client,
            id: &str,
        ) -> crate::Result<crate::types::HostView> {
            client.call(&crate::endpoints::GetV1HostsId::new(id))
        }

        /// List live hosts in the pool
        pub fn list(
            client: &crate::blocking::Client,
        ) -> crate::Result<crate::types::HostsResponse> {
            client.call(&crate::endpoints::GetV1Hosts::new())
        }

        /// Mint a ten-minute single-use host enrollment token
        pub fn mint_enrollment_token(
            client: &crate::blocking::Client,
            body: crate::types::MintEnrollmentTokenRequest,
        ) -> crate::Result<crate::types::MintEnrollmentTokenResponse> {
            client.call(&crate::endpoints::PostV1HostsEnrollmentTokens::new(body))
        }

        /// Revoke the pool relationship for a host
        pub fn remove(client: &crate::blocking::Client, id: &str) -> crate::Result<()> {
            client.call(&crate::endpoints::DeleteV1HostsId::new(id))
        }

        /// Rename a host slug or display name; the id is unchanged
        pub fn rename(
            client: &crate::blocking::Client,
            id: &str,
            body: crate::types::RenameHostRequest,
        ) -> crate::Result<crate::types::HostView> {
            client.call(&crate::endpoints::PatchV1HostsId::new(id, body))
        }

        /// Host reports providers, models, modes and thinking options
        pub fn report_capabilities(
            client: &crate::blocking::Client,
            id: &str,
            body: crate::types::CapabilityReportRequest,
        ) -> crate::Result<crate::types::HostCapabilityReport> {
            client.call(&crate::endpoints::PostV1HostsIdCapabilities::new(id, body))
        }

        /// Grant host.execute so the host may run agents
        pub fn start_execution(
            client: &crate::blocking::Client,
            id: &str,
            body: crate::types::JsonValue,
        ) -> crate::Result<crate::types::HostView> {
            client.call(&crate::endpoints::PostV1HostsIdExecution::new(id, body))
        }

        /// Revoke host.execute; the host becomes connected-only
        pub fn stop_execution(
            client: &crate::blocking::Client,
            id: &str,
        ) -> crate::Result<crate::types::HostView> {
            client.call(&crate::endpoints::DeleteV1HostsIdExecution::new(id))
        }
    }

    /// `pool` resources.
    pub mod pool {
        /// Accept an invitation and create the member account
        pub fn accept_invitation(
            client: &crate::blocking::Client,
            body: crate::types::AcceptInvitationRequest,
        ) -> crate::Result<crate::types::PoolLoginResponse> {
            client.call(&crate::endpoints::PostV1PoolInvitationsAccept::new(body))
        }

        /// Replace the operator's password (clears first-sign-in requirement)
        pub fn change_password(
            client: &crate::blocking::Client,
            body: crate::types::ChangePasswordRequest,
        ) -> crate::Result<()> {
            client.call(&crate::endpoints::PostV1PoolPassword::new(body))
        }

        /// List pending team invitations (owner only)
        pub fn invitations(
            client: &crate::blocking::Client,
        ) -> crate::Result<crate::types::InvitationsResponse> {
            client.call(&crate::endpoints::GetV1PoolInvitations::new())
        }

        /// Invite an operator by email; the token is shown once
        pub fn invite(
            client: &crate::blocking::Client,
            body: crate::types::InviteRequest,
        ) -> crate::Result<crate::types::InviteResponse> {
            client.call(&crate::endpoints::PostV1PoolInvitations::new(body))
        }

        /// Sign an operator in and issue a JWT
        pub fn login(
            client: &crate::blocking::Client,
            body: crate::types::PoolLoginRequest,
        ) -> crate::Result<crate::types::PoolLoginResponse> {
            client.call(&crate::endpoints::PostV1PoolLogin::new(body))
        }

        /// Fetch the signed-in operator and organization
        pub fn me(
            client: &crate::blocking::Client,
        ) -> crate::Result<crate::types::PoolIdentityView> {
            client.call(&crate::endpoints::GetV1PoolMe::new())
        }

        /// List the organization's operators
        pub fn members(
            client: &crate::blocking::Client,
        ) -> crate::Result<crate::types::MembersResponse> {
            client.call(&crate::endpoints::GetV1PoolMembers::new())
        }

        /// Remove a member (owner only; last owner protected)
        pub fn remove_member(client: &crate::blocking::Client, id: &str) -> crate::Result<()> {
            client.call(&crate::endpoints::DeleteV1PoolMembersId::new(id))
        }

        /// Revoke a pending invitation (owner only)
        pub fn revoke_invitation(client: &crate::blocking::Client, id: &str) -> crate::Result<()> {
            client.call(&crate::endpoints::DeleteV1PoolInvitationsId::new(id))
        }

        /// Change a member's role (owner only; last owner protected)
        pub fn update_member(
            client: &crate::blocking::Client,
            id: &str,
            body: crate::types::UpdateRoleRequest,
        ) -> crate::Result<crate::types::PoolUserView> {
            client.call(&crate::endpoints::PatchV1PoolMembersId::new(id, body))
        }
    }

    /// `projects` resources.
    pub mod projects {
        /// Fetch a project's active configuration revision
        pub fn configuration(
            client: &crate::blocking::Client,
            id: &str,
        ) -> crate::Result<crate::types::RevisionView> {
            client.call(&crate::endpoints::GetV1ProjectsIdConfiguration::new(id))
        }

        /// Create a project
        pub fn create(
            client: &crate::blocking::Client,
            body: crate::types::CreateProjectRequest,
        ) -> crate::Result<crate::types::ProjectView> {
            client.call(&crate::endpoints::PostV1Projects::new(body))
        }

        /// Fetch a project by id
        pub fn get(
            client: &crate::blocking::Client,
            id: &str,
        ) -> crate::Result<crate::types::ProjectView> {
            client.call(&crate::endpoints::GetV1ProjectsId::new(id))
        }

        /// Install a configuration bundle as a new immutable revision
        pub fn install_configuration(
            client: &crate::blocking::Client,
            id: &str,
            body: crate::types::ConfigurationRequest,
        ) -> crate::Result<crate::types::InstallResponse> {
            client.call(&crate::endpoints::PostV1ProjectsIdConfigurationInstall::new(id, body))
        }

        /// List projects in the organization
        pub fn list(
            client: &crate::blocking::Client,
        ) -> crate::Result<crate::types::ProjectsResponse> {
            client.call(&crate::endpoints::GetV1Projects::new())
        }

        /// Delete a project and its revisions
        pub fn remove(client: &crate::blocking::Client, id: &str) -> crate::Result<()> {
            client.call(&crate::endpoints::DeleteV1ProjectsId::new(id))
        }

        /// Rename a project slug or name; the id is unchanged
        pub fn rename(
            client: &crate::blocking::Client,
            id: &str,
            body: crate::types::RenameProjectRequest,
        ) -> crate::Result<crate::types::ProjectView> {
            client.call(&crate::endpoints::PatchV1ProjectsId::new(id, body))
        }

        /// Fetch one configuration revision with its files
        pub fn revision(
            client: &crate::blocking::Client,
            id: &str,
            revision_id: &str,
        ) -> crate::Result<crate::types::RevisionView> {
            client.call(&crate::endpoints::GetV1ProjectsIdRevisionsRevisionId::new(
                id,
                revision_id,
            ))
        }

        /// List a project's configuration revisions
        pub fn revisions(
            client: &crate::blocking::Client,
            id: &str,
        ) -> crate::Result<crate::types::RevisionsResponse> {
            client.call(&crate::endpoints::GetV1ProjectsIdRevisions::new(id))
        }

        /// Validate a configuration bundle without writing pool state
        pub fn validate_configuration(
            client: &crate::blocking::Client,
            id: &str,
            body: crate::types::ConfigurationRequest,
        ) -> crate::Result<crate::types::ValidateResponse> {
            client.call(&crate::endpoints::PostV1ProjectsIdConfigurationValidate::new(id, body))
        }
    }

    /// `relay` resources.
    pub mod relay {
        /// Opt a session into publication
        pub fn enable(
            client: &crate::blocking::Client,
            session: &str,
            body: crate::types::JsonValue,
        ) -> crate::Result<crate::types::RelayState> {
            client.call(&crate::endpoints::PutV1SessionsSessionRelay::new(
                session, body,
            ))
        }

        /// Backfill relayed events after a sequence cursor
        pub fn events(
            client: &crate::blocking::Client,
            session: &str,
            query: crate::endpoints::GetV1SessionsSessionRelayEventsQuery,
        ) -> crate::Result<crate::types::RelayPage> {
            client.call(
                &crate::endpoints::GetV1SessionsSessionRelayEvents::new(session).with_query(query),
            )
        }

        /// Read a session's relay publication state
        pub fn get(
            client: &crate::blocking::Client,
            session: &str,
        ) -> crate::Result<crate::types::RelayState> {
            client.call(&crate::endpoints::GetV1SessionsSessionRelay::new(session))
        }

        /// Publish one allow-listed session event
        pub fn publish(
            client: &crate::blocking::Client,
            session: &str,
            body: crate::types::PublishBody,
        ) -> crate::Result<crate::types::RelayEvent> {
            client.call(&crate::endpoints::PostV1SessionsSessionRelayEvents::new(
                session, body,
            ))
        }

        /// Stop publication immediately and permanently
        pub fn stop(
            client: &crate::blocking::Client,
            session: &str,
        ) -> crate::Result<crate::types::RelayState> {
            client.call(&crate::endpoints::DeleteV1SessionsSessionRelay::new(
                session,
            ))
        }
    }

    /// `runs` resources.
    pub mod runs {
        /// Dispatch a workflow run from a manual request
        pub fn dispatch(
            client: &crate::blocking::Client,
            body: crate::types::ManualDispatchRequest,
        ) -> crate::Result<crate::types::ManualDispatchResponse> {
            client.call(&crate::endpoints::PostV1Runs::new(body))
        }
    }

    /// `setup` resources.
    pub mod setup {
        /// Create the organization and first owner from the browser
        pub fn create(
            client: &crate::blocking::Client,
            body: crate::types::SetupRequest,
        ) -> crate::Result<crate::types::PoolIdentityView> {
            client.call(&crate::endpoints::PostV1Setup::new(body))
        }

        /// Report whether the pool still needs first-run setup
        pub fn status(
            client: &crate::blocking::Client,
        ) -> crate::Result<crate::types::SetupStatusResponse> {
            client.call(&crate::endpoints::GetV1SetupStatus::new())
        }
    }

    /// `system` resources.
    pub mod system {
        /// Liveness probe with DB status
        pub fn health(client: &crate::blocking::Client) -> crate::Result<crate::types::JsonObject> {
            client.call(&crate::endpoints::GetHealthz::new())
        }

        /// Prometheus text-format metrics
        pub fn metrics(client: &crate::blocking::Client) -> crate::Result<String> {
            client.call(&crate::endpoints::GetMetrics::new())
        }

        /// This OpenAPI document
        pub fn openapi(
            client: &crate::blocking::Client,
        ) -> crate::Result<crate::types::OpenApiDocument> {
            client.call(&crate::endpoints::GetOpenapiJson::new())
        }

        /// This OpenAPI document at its pinned major-version path
        pub fn openapi_v1(
            client: &crate::blocking::Client,
        ) -> crate::Result<crate::types::OpenApiDocument> {
            client.call(&crate::endpoints::GetOpenapiV1Json::new())
        }
    }

    /// `triggers` resources.
    pub mod triggers {
        /// Install or replace a trigger; keeps its id and bumps the revision
        pub fn install(
            client: &crate::blocking::Client,
            body: crate::types::TriggerInstallRequest,
        ) -> crate::Result<crate::types::TriggerInstallResponse> {
            client.call(&crate::endpoints::PostV1TriggersInstall::new(body))
        }

        /// Validate one trigger document without writing pool state
        pub fn validate(
            client: &crate::blocking::Client,
            body: crate::types::TriggerDocument,
        ) -> crate::Result<crate::types::TriggerValidateResponse> {
            client.call(&crate::endpoints::PostV1TriggersValidate::new(body))
        }
    }

    /// `webhooks` resources.
    pub mod webhooks {
        /// Receive a GitHub App webhook; signature verified and delivery deduplicated
        pub fn github(
            client: &crate::blocking::Client,
            slug: &str,
            body: crate::types::RawJson,
        ) -> crate::Result<crate::types::WebhookResponse> {
            client.call(&crate::endpoints::PostV1WebhooksGithubSlug::new(slug, body))
        }

        /// Receive a Slack Events API callback; signature verified and event deduplicated
        pub fn slack(
            client: &crate::blocking::Client,
            slug: &str,
            body: crate::types::RawJson,
        ) -> crate::Result<crate::endpoints::PostV1WebhooksSlackSlugResponse> {
            client.call(&crate::endpoints::PostV1WebhooksSlackSlug::new(slug, body))
        }
    }
}

#[cfg(feature = "async")]
pub mod async_api {
    //! Typed async calls: one function per contract route.

    /// `activity` resources.
    pub mod activity {
        /// List one page of receipts, runs, steps and executions
        pub async fn list(
            client: &crate::asynchronous::Client,
            query: crate::endpoints::GetV1ActivityQuery,
        ) -> crate::Result<crate::types::ActivityPage> {
            client
                .call(&crate::endpoints::GetV1Activity::new().with_query(query))
                .await
        }

        /// Fetch one run with the arrival record that started it
        pub async fn run(
            client: &crate::asynchronous::Client,
            run: &str,
        ) -> crate::Result<crate::types::ActivityRunDetail> {
            client
                .call(&crate::endpoints::GetV1ActivityRunsRun::new(run))
                .await
        }
    }

    /// `apiKeys` resources.
    pub mod api_keys {
        /// Create an API key; the full secret is shown once
        pub async fn create(
            client: &crate::asynchronous::Client,
            body: crate::types::CreateApiKeyRequest,
        ) -> crate::Result<crate::types::CreateApiKeyResponse> {
            client
                .call(&crate::endpoints::PostV1ApiKeys::new(body))
                .await
        }

        /// List organization API keys (owner only; prefix only)
        pub async fn list(
            client: &crate::asynchronous::Client,
        ) -> crate::Result<crate::types::ApiKeysResponse> {
            client.call(&crate::endpoints::GetV1ApiKeys::new()).await
        }

        /// Revoke an API key (owner only; effective immediately)
        pub async fn remove(client: &crate::asynchronous::Client, id: &str) -> crate::Result<()> {
            client
                .call(&crate::endpoints::DeleteV1ApiKeysId::new(id))
                .await
        }

        /// Introspect an API key (requires projects:read)
        pub async fn whoami(
            client: &crate::asynchronous::Client,
        ) -> crate::Result<crate::types::ApiKeyView> {
            client
                .call(&crate::endpoints::GetV1ApiKeysWhoami::new())
                .await
        }
    }

    /// `chatrooms` resources.
    pub mod chatrooms {
        /// List rooms visible to the operator
        pub async fn list(
            client: &crate::asynchronous::Client,
        ) -> crate::Result<crate::types::RoomViewList> {
            client.call(&crate::endpoints::GetV1Chatrooms::new()).await
        }

        /// List a room's members
        pub async fn members(
            client: &crate::asynchronous::Client,
            room: &str,
        ) -> crate::Result<crate::types::RoomMemberViewList> {
            client
                .call(&crate::endpoints::GetV1ChatroomsRoomMembers::new(room))
                .await
        }

        /// Read room history after a sequence cursor
        pub async fn messages(
            client: &crate::asynchronous::Client,
            room: &str,
            query: crate::endpoints::GetV1ChatroomsRoomMessagesQuery,
        ) -> crate::Result<crate::types::HistoryResponse> {
            client
                .call(&crate::endpoints::GetV1ChatroomsRoomMessages::new(room).with_query(query))
                .await
        }

        /// Post a message to a room
        pub async fn send(
            client: &crate::asynchronous::Client,
            room: &str,
            body: crate::types::SendMessageBody,
        ) -> crate::Result<crate::types::MessageView> {
            client
                .call(&crate::endpoints::PostV1ChatroomsRoomMessages::new(
                    room, body,
                ))
                .await
        }

        /// Upgrade to the room's live message stream
        pub async fn stream(
            client: &crate::asynchronous::Client,
            room: &str,
            query: crate::endpoints::GetV1ChatroomsRoomStreamQuery,
        ) -> crate::Result<crate::stream::ChatStream> {
            client.stream(&crate::endpoints::GetV1ChatroomsRoomStream::new(room).with_query(query))
        }
    }

    /// `connections` resources.
    pub mod connections {
        /// Create a provider connection from a credential reference
        pub async fn create(
            client: &crate::asynchronous::Client,
            body: crate::types::CreateConnectionRequest,
        ) -> crate::Result<crate::types::ConnectionView> {
            client
                .call(&crate::endpoints::PostV1Connections::new(body))
                .await
        }

        /// Fetch a connection by id or env:<slug>
        pub async fn get(
            client: &crate::asynchronous::Client,
            id: &str,
        ) -> crate::Result<crate::types::ConnectionView> {
            client
                .call(&crate::endpoints::GetV1ConnectionsId::new(id))
                .await
        }

        /// List effective provider connections (environment overlay applied)
        pub async fn list(
            client: &crate::asynchronous::Client,
        ) -> crate::Result<crate::types::ConnectionsResponse> {
            client
                .call(&crate::endpoints::GetV1Connections::new())
                .await
        }

        /// Delete a database connection
        pub async fn remove(client: &crate::asynchronous::Client, id: &str) -> crate::Result<()> {
            client
                .call(&crate::endpoints::DeleteV1ConnectionsId::new(id))
                .await
        }

        /// Rename, enable or disable a database connection
        pub async fn update(
            client: &crate::asynchronous::Client,
            id: &str,
            body: crate::types::UpdateConnectionRequest,
        ) -> crate::Result<crate::types::ConnectionView> {
            client
                .call(&crate::endpoints::PatchV1ConnectionsId::new(id, body))
                .await
        }
    }

    /// `game` resources.
    pub mod game {
        /// `auth` resources.
        pub mod auth {
            /// Verify credentials and issue a signed JWT
            pub async fn login(
                client: &crate::asynchronous::Client,
                body: crate::types::JsonValue,
            ) -> crate::Result<crate::types::JsonObject> {
                client
                    .call(&crate::endpoints::PostV1AuthLogin::new(body))
                    .await
            }

            /// Register a new player and return the public profile
            pub async fn register(
                client: &crate::asynchronous::Client,
                body: crate::types::JsonValue,
            ) -> crate::Result<crate::types::JsonObject> {
                client
                    .call(&crate::endpoints::PostV1AuthRegister::new(body))
                    .await
            }
        }

        /// `events` resources.
        pub mod events {
            /// Ingest a batch of analytics events (fire-and-forget)
            pub async fn create(
                client: &crate::asynchronous::Client,
                body: crate::types::JsonValue,
            ) -> crate::Result<crate::types::JsonObject> {
                client
                    .call(&crate::endpoints::PostV1Events::new(body))
                    .await
            }
        }

        /// `feedback` resources.
        pub mod feedback {
            /// Submit player feedback
            pub async fn create(
                client: &crate::asynchronous::Client,
                body: crate::types::JsonValue,
            ) -> crate::Result<crate::types::JsonObject> {
                client
                    .call(&crate::endpoints::PostV1Feedback::new(body))
                    .await
            }
        }

        /// `games` resources.
        pub mod games {
            /// Fetch a game's settings
            pub async fn settings(
                client: &crate::asynchronous::Client,
                game_id: &str,
            ) -> crate::Result<crate::types::JsonObject> {
                client
                    .call(&crate::endpoints::GetV1GamesGameIdSettings::new(game_id))
                    .await
            }

            /// Set a game's settings (upsert)
            pub async fn update_settings(
                client: &crate::asynchronous::Client,
                game_id: &str,
                body: crate::types::JsonValue,
            ) -> crate::Result<crate::types::JsonObject> {
                client
                    .call(&crate::endpoints::PutV1GamesGameIdSettings::new(
                        game_id, body,
                    ))
                    .await
            }
        }

        /// `leaderboards` resources.
        pub mod leaderboards {
            /// Fetch one ranked page of a leaderboard
            pub async fn get(
                client: &crate::asynchronous::Client,
                game_id: &str,
                query: crate::endpoints::GetV1LeaderboardsGameIdQuery,
            ) -> crate::Result<crate::types::JsonObject> {
                client
                    .call(
                        &crate::endpoints::GetV1LeaderboardsGameId::new(game_id).with_query(query),
                    )
                    .await
            }

            /// Update the authenticated player's own leaderboard entry
            pub async fn put_entry(
                client: &crate::asynchronous::Client,
                game_id: &str,
                player_id: &str,
                body: crate::types::JsonValue,
            ) -> crate::Result<crate::types::JsonObject> {
                client
                    .call(
                        &crate::endpoints::PutV1LeaderboardsGameIdEntriesPlayerId::new(
                            game_id, player_id, body,
                        ),
                    )
                    .await
            }

            /// Submit a leaderboard entry for the authenticated player
            pub async fn submit_entry(
                client: &crate::asynchronous::Client,
                game_id: &str,
                body: crate::types::JsonValue,
            ) -> crate::Result<crate::types::JsonObject> {
                client
                    .call(&crate::endpoints::PostV1LeaderboardsGameIdEntries::new(
                        game_id, body,
                    ))
                    .await
            }
        }

        /// `players` resources.
        pub mod players {
            /// Create a subaccount under the authenticated player
            pub async fn create_subaccount(
                client: &crate::asynchronous::Client,
                body: crate::types::JsonValue,
            ) -> crate::Result<crate::types::JsonObject> {
                client
                    .call(&crate::endpoints::PostV1PlayersSubaccount::new(body))
                    .await
            }

            /// Fetch a player's public profile
            pub async fn get(
                client: &crate::asynchronous::Client,
                id: &str,
            ) -> crate::Result<crate::types::JsonObject> {
                client
                    .call(&crate::endpoints::GetV1PlayersId::new(id))
                    .await
            }

            /// Soft-delete the authenticated player's own account
            pub async fn remove(
                client: &crate::asynchronous::Client,
                id: &str,
            ) -> crate::Result<()> {
                client
                    .call(&crate::endpoints::DeleteV1PlayersId::new(id))
                    .await
            }

            /// List the authenticated player's subaccounts
            pub async fn subaccounts(
                client: &crate::asynchronous::Client,
                id: &str,
            ) -> crate::Result<crate::types::JsonObject> {
                client
                    .call(&crate::endpoints::GetV1PlayersIdSubaccounts::new(id))
                    .await
            }

            /// Update the authenticated player's own profile
            pub async fn update(
                client: &crate::asynchronous::Client,
                id: &str,
                body: crate::types::JsonValue,
            ) -> crate::Result<crate::types::JsonObject> {
                client
                    .call(&crate::endpoints::PutV1PlayersId::new(id, body))
                    .await
            }
        }

        /// `saves` resources.
        pub mod saves {
            /// Create a game save
            pub async fn create(
                client: &crate::asynchronous::Client,
                body: crate::types::JsonValue,
            ) -> crate::Result<crate::types::JsonObject> {
                client.call(&crate::endpoints::PostV1Saves::new(body)).await
            }

            /// Fetch a single game save
            pub async fn get(
                client: &crate::asynchronous::Client,
                player_id: &str,
                save_id: &str,
            ) -> crate::Result<crate::types::JsonObject> {
                client
                    .call(&crate::endpoints::GetV1SavesPlayerIdSaveId::new(
                        player_id, save_id,
                    ))
                    .await
            }

            /// List the authenticated player's saves, newest first
            pub async fn list(
                client: &crate::asynchronous::Client,
                player_id: &str,
            ) -> crate::Result<crate::types::JsonObject> {
                client
                    .call(&crate::endpoints::GetV1SavesPlayerId::new(player_id))
                    .await
            }

            /// Delete a single game save
            pub async fn remove(
                client: &crate::asynchronous::Client,
                player_id: &str,
                save_id: &str,
            ) -> crate::Result<()> {
                client
                    .call(&crate::endpoints::DeleteV1SavesPlayerIdSaveId::new(
                        player_id, save_id,
                    ))
                    .await
            }
        }

        /// `socketTickets` resources.
        pub mod socket_tickets {
            /// Issue a one-shot WebSocket connection ticket
            pub async fn create(
                client: &crate::asynchronous::Client,
                body: crate::types::JsonValue,
            ) -> crate::Result<crate::types::JsonObject> {
                client
                    .call(&crate::endpoints::PostV1SocketTickets::new(body))
                    .await
            }
        }
    }

    /// `hosts` resources.
    pub mod hosts {
        /// Read the host's last capability report and offered providers
        pub async fn capabilities(
            client: &crate::asynchronous::Client,
            id: &str,
        ) -> crate::Result<crate::types::HostCapabilityReport> {
            client
                .call(&crate::endpoints::GetV1HostsIdCapabilities::new(id))
                .await
        }

        /// Register a host; the slug is derived from the hostname
        pub async fn create(
            client: &crate::asynchronous::Client,
            body: crate::types::CreateHostRequest,
        ) -> crate::Result<crate::types::HostView> {
            client.call(&crate::endpoints::PostV1Hosts::new(body)).await
        }

        /// Create or replay a create-agent intent with a stable execution id
        pub async fn create_execution(
            client: &crate::asynchronous::Client,
            id: &str,
            body: crate::types::CreateExecutionRequest,
        ) -> crate::Result<crate::types::CreateExecutionResponse> {
            client
                .call(&crate::endpoints::PostV1HostsIdExecutions::new(id, body))
                .await
        }

        /// Exchange an enrollment token for a host relationship credential
        pub async fn enroll(
            client: &crate::asynchronous::Client,
            body: crate::types::EnrollHostRequest,
        ) -> crate::Result<crate::types::HostView> {
            client
                .call(&crate::endpoints::PostV1HostsEnroll::new(body))
                .await
        }

        /// Fetch a live host by immutable id
        pub async fn get(
            client: &crate::asynchronous::Client,
            id: &str,
        ) -> crate::Result<crate::types::HostView> {
            client.call(&crate::endpoints::GetV1HostsId::new(id)).await
        }

        /// List live hosts in the pool
        pub async fn list(
            client: &crate::asynchronous::Client,
        ) -> crate::Result<crate::types::HostsResponse> {
            client.call(&crate::endpoints::GetV1Hosts::new()).await
        }

        /// Mint a ten-minute single-use host enrollment token
        pub async fn mint_enrollment_token(
            client: &crate::asynchronous::Client,
            body: crate::types::MintEnrollmentTokenRequest,
        ) -> crate::Result<crate::types::MintEnrollmentTokenResponse> {
            client
                .call(&crate::endpoints::PostV1HostsEnrollmentTokens::new(body))
                .await
        }

        /// Revoke the pool relationship for a host
        pub async fn remove(client: &crate::asynchronous::Client, id: &str) -> crate::Result<()> {
            client
                .call(&crate::endpoints::DeleteV1HostsId::new(id))
                .await
        }

        /// Rename a host slug or display name; the id is unchanged
        pub async fn rename(
            client: &crate::asynchronous::Client,
            id: &str,
            body: crate::types::RenameHostRequest,
        ) -> crate::Result<crate::types::HostView> {
            client
                .call(&crate::endpoints::PatchV1HostsId::new(id, body))
                .await
        }

        /// Host reports providers, models, modes and thinking options
        pub async fn report_capabilities(
            client: &crate::asynchronous::Client,
            id: &str,
            body: crate::types::CapabilityReportRequest,
        ) -> crate::Result<crate::types::HostCapabilityReport> {
            client
                .call(&crate::endpoints::PostV1HostsIdCapabilities::new(id, body))
                .await
        }

        /// Grant host.execute so the host may run agents
        pub async fn start_execution(
            client: &crate::asynchronous::Client,
            id: &str,
            body: crate::types::JsonValue,
        ) -> crate::Result<crate::types::HostView> {
            client
                .call(&crate::endpoints::PostV1HostsIdExecution::new(id, body))
                .await
        }

        /// Revoke host.execute; the host becomes connected-only
        pub async fn stop_execution(
            client: &crate::asynchronous::Client,
            id: &str,
        ) -> crate::Result<crate::types::HostView> {
            client
                .call(&crate::endpoints::DeleteV1HostsIdExecution::new(id))
                .await
        }
    }

    /// `pool` resources.
    pub mod pool {
        /// Accept an invitation and create the member account
        pub async fn accept_invitation(
            client: &crate::asynchronous::Client,
            body: crate::types::AcceptInvitationRequest,
        ) -> crate::Result<crate::types::PoolLoginResponse> {
            client
                .call(&crate::endpoints::PostV1PoolInvitationsAccept::new(body))
                .await
        }

        /// Replace the operator's password (clears first-sign-in requirement)
        pub async fn change_password(
            client: &crate::asynchronous::Client,
            body: crate::types::ChangePasswordRequest,
        ) -> crate::Result<()> {
            client
                .call(&crate::endpoints::PostV1PoolPassword::new(body))
                .await
        }

        /// List pending team invitations (owner only)
        pub async fn invitations(
            client: &crate::asynchronous::Client,
        ) -> crate::Result<crate::types::InvitationsResponse> {
            client
                .call(&crate::endpoints::GetV1PoolInvitations::new())
                .await
        }

        /// Invite an operator by email; the token is shown once
        pub async fn invite(
            client: &crate::asynchronous::Client,
            body: crate::types::InviteRequest,
        ) -> crate::Result<crate::types::InviteResponse> {
            client
                .call(&crate::endpoints::PostV1PoolInvitations::new(body))
                .await
        }

        /// Sign an operator in and issue a JWT
        pub async fn login(
            client: &crate::asynchronous::Client,
            body: crate::types::PoolLoginRequest,
        ) -> crate::Result<crate::types::PoolLoginResponse> {
            client
                .call(&crate::endpoints::PostV1PoolLogin::new(body))
                .await
        }

        /// Fetch the signed-in operator and organization
        pub async fn me(
            client: &crate::asynchronous::Client,
        ) -> crate::Result<crate::types::PoolIdentityView> {
            client.call(&crate::endpoints::GetV1PoolMe::new()).await
        }

        /// List the organization's operators
        pub async fn members(
            client: &crate::asynchronous::Client,
        ) -> crate::Result<crate::types::MembersResponse> {
            client
                .call(&crate::endpoints::GetV1PoolMembers::new())
                .await
        }

        /// Remove a member (owner only; last owner protected)
        pub async fn remove_member(
            client: &crate::asynchronous::Client,
            id: &str,
        ) -> crate::Result<()> {
            client
                .call(&crate::endpoints::DeleteV1PoolMembersId::new(id))
                .await
        }

        /// Revoke a pending invitation (owner only)
        pub async fn revoke_invitation(
            client: &crate::asynchronous::Client,
            id: &str,
        ) -> crate::Result<()> {
            client
                .call(&crate::endpoints::DeleteV1PoolInvitationsId::new(id))
                .await
        }

        /// Change a member's role (owner only; last owner protected)
        pub async fn update_member(
            client: &crate::asynchronous::Client,
            id: &str,
            body: crate::types::UpdateRoleRequest,
        ) -> crate::Result<crate::types::PoolUserView> {
            client
                .call(&crate::endpoints::PatchV1PoolMembersId::new(id, body))
                .await
        }
    }

    /// `projects` resources.
    pub mod projects {
        /// Fetch a project's active configuration revision
        pub async fn configuration(
            client: &crate::asynchronous::Client,
            id: &str,
        ) -> crate::Result<crate::types::RevisionView> {
            client
                .call(&crate::endpoints::GetV1ProjectsIdConfiguration::new(id))
                .await
        }

        /// Create a project
        pub async fn create(
            client: &crate::asynchronous::Client,
            body: crate::types::CreateProjectRequest,
        ) -> crate::Result<crate::types::ProjectView> {
            client
                .call(&crate::endpoints::PostV1Projects::new(body))
                .await
        }

        /// Fetch a project by id
        pub async fn get(
            client: &crate::asynchronous::Client,
            id: &str,
        ) -> crate::Result<crate::types::ProjectView> {
            client
                .call(&crate::endpoints::GetV1ProjectsId::new(id))
                .await
        }

        /// Install a configuration bundle as a new immutable revision
        pub async fn install_configuration(
            client: &crate::asynchronous::Client,
            id: &str,
            body: crate::types::ConfigurationRequest,
        ) -> crate::Result<crate::types::InstallResponse> {
            client
                .call(&crate::endpoints::PostV1ProjectsIdConfigurationInstall::new(id, body))
                .await
        }

        /// List projects in the organization
        pub async fn list(
            client: &crate::asynchronous::Client,
        ) -> crate::Result<crate::types::ProjectsResponse> {
            client.call(&crate::endpoints::GetV1Projects::new()).await
        }

        /// Delete a project and its revisions
        pub async fn remove(client: &crate::asynchronous::Client, id: &str) -> crate::Result<()> {
            client
                .call(&crate::endpoints::DeleteV1ProjectsId::new(id))
                .await
        }

        /// Rename a project slug or name; the id is unchanged
        pub async fn rename(
            client: &crate::asynchronous::Client,
            id: &str,
            body: crate::types::RenameProjectRequest,
        ) -> crate::Result<crate::types::ProjectView> {
            client
                .call(&crate::endpoints::PatchV1ProjectsId::new(id, body))
                .await
        }

        /// Fetch one configuration revision with its files
        pub async fn revision(
            client: &crate::asynchronous::Client,
            id: &str,
            revision_id: &str,
        ) -> crate::Result<crate::types::RevisionView> {
            client
                .call(&crate::endpoints::GetV1ProjectsIdRevisionsRevisionId::new(
                    id,
                    revision_id,
                ))
                .await
        }

        /// List a project's configuration revisions
        pub async fn revisions(
            client: &crate::asynchronous::Client,
            id: &str,
        ) -> crate::Result<crate::types::RevisionsResponse> {
            client
                .call(&crate::endpoints::GetV1ProjectsIdRevisions::new(id))
                .await
        }

        /// Validate a configuration bundle without writing pool state
        pub async fn validate_configuration(
            client: &crate::asynchronous::Client,
            id: &str,
            body: crate::types::ConfigurationRequest,
        ) -> crate::Result<crate::types::ValidateResponse> {
            client
                .call(&crate::endpoints::PostV1ProjectsIdConfigurationValidate::new(id, body))
                .await
        }
    }

    /// `relay` resources.
    pub mod relay {
        /// Opt a session into publication
        pub async fn enable(
            client: &crate::asynchronous::Client,
            session: &str,
            body: crate::types::JsonValue,
        ) -> crate::Result<crate::types::RelayState> {
            client
                .call(&crate::endpoints::PutV1SessionsSessionRelay::new(
                    session, body,
                ))
                .await
        }

        /// Backfill relayed events after a sequence cursor
        pub async fn events(
            client: &crate::asynchronous::Client,
            session: &str,
            query: crate::endpoints::GetV1SessionsSessionRelayEventsQuery,
        ) -> crate::Result<crate::types::RelayPage> {
            client
                .call(
                    &crate::endpoints::GetV1SessionsSessionRelayEvents::new(session)
                        .with_query(query),
                )
                .await
        }

        /// Read a session's relay publication state
        pub async fn get(
            client: &crate::asynchronous::Client,
            session: &str,
        ) -> crate::Result<crate::types::RelayState> {
            client
                .call(&crate::endpoints::GetV1SessionsSessionRelay::new(session))
                .await
        }

        /// Publish one allow-listed session event
        pub async fn publish(
            client: &crate::asynchronous::Client,
            session: &str,
            body: crate::types::PublishBody,
        ) -> crate::Result<crate::types::RelayEvent> {
            client
                .call(&crate::endpoints::PostV1SessionsSessionRelayEvents::new(
                    session, body,
                ))
                .await
        }

        /// Stop publication immediately and permanently
        pub async fn stop(
            client: &crate::asynchronous::Client,
            session: &str,
        ) -> crate::Result<crate::types::RelayState> {
            client
                .call(&crate::endpoints::DeleteV1SessionsSessionRelay::new(
                    session,
                ))
                .await
        }
    }

    /// `runs` resources.
    pub mod runs {
        /// Dispatch a workflow run from a manual request
        pub async fn dispatch(
            client: &crate::asynchronous::Client,
            body: crate::types::ManualDispatchRequest,
        ) -> crate::Result<crate::types::ManualDispatchResponse> {
            client.call(&crate::endpoints::PostV1Runs::new(body)).await
        }
    }

    /// `setup` resources.
    pub mod setup {
        /// Create the organization and first owner from the browser
        pub async fn create(
            client: &crate::asynchronous::Client,
            body: crate::types::SetupRequest,
        ) -> crate::Result<crate::types::PoolIdentityView> {
            client.call(&crate::endpoints::PostV1Setup::new(body)).await
        }

        /// Report whether the pool still needs first-run setup
        pub async fn status(
            client: &crate::asynchronous::Client,
        ) -> crate::Result<crate::types::SetupStatusResponse> {
            client
                .call(&crate::endpoints::GetV1SetupStatus::new())
                .await
        }
    }

    /// `system` resources.
    pub mod system {
        /// Liveness probe with DB status
        pub async fn health(
            client: &crate::asynchronous::Client,
        ) -> crate::Result<crate::types::JsonObject> {
            client.call(&crate::endpoints::GetHealthz::new()).await
        }

        /// Prometheus text-format metrics
        pub async fn metrics(client: &crate::asynchronous::Client) -> crate::Result<String> {
            client.call(&crate::endpoints::GetMetrics::new()).await
        }

        /// This OpenAPI document
        pub async fn openapi(
            client: &crate::asynchronous::Client,
        ) -> crate::Result<crate::types::OpenApiDocument> {
            client.call(&crate::endpoints::GetOpenapiJson::new()).await
        }

        /// This OpenAPI document at its pinned major-version path
        pub async fn openapi_v1(
            client: &crate::asynchronous::Client,
        ) -> crate::Result<crate::types::OpenApiDocument> {
            client
                .call(&crate::endpoints::GetOpenapiV1Json::new())
                .await
        }
    }

    /// `triggers` resources.
    pub mod triggers {
        /// Install or replace a trigger; keeps its id and bumps the revision
        pub async fn install(
            client: &crate::asynchronous::Client,
            body: crate::types::TriggerInstallRequest,
        ) -> crate::Result<crate::types::TriggerInstallResponse> {
            client
                .call(&crate::endpoints::PostV1TriggersInstall::new(body))
                .await
        }

        /// Validate one trigger document without writing pool state
        pub async fn validate(
            client: &crate::asynchronous::Client,
            body: crate::types::TriggerDocument,
        ) -> crate::Result<crate::types::TriggerValidateResponse> {
            client
                .call(&crate::endpoints::PostV1TriggersValidate::new(body))
                .await
        }
    }

    /// `webhooks` resources.
    pub mod webhooks {
        /// Receive a GitHub App webhook; signature verified and delivery deduplicated
        pub async fn github(
            client: &crate::asynchronous::Client,
            slug: &str,
            body: crate::types::RawJson,
        ) -> crate::Result<crate::types::WebhookResponse> {
            client
                .call(&crate::endpoints::PostV1WebhooksGithubSlug::new(slug, body))
                .await
        }

        /// Receive a Slack Events API callback; signature verified and event deduplicated
        pub async fn slack(
            client: &crate::asynchronous::Client,
            slug: &str,
            body: crate::types::RawJson,
        ) -> crate::Result<crate::endpoints::PostV1WebhooksSlackSlugResponse> {
            client
                .call(&crate::endpoints::PostV1WebhooksSlackSlug::new(slug, body))
                .await
        }
    }
}
