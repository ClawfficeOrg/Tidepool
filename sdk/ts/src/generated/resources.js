// Generated from docs/api/openapi-v1.json. Do not edit by hand.
// Regenerate: npm run generate (from sdk/ts).

/**
 * Bind every typed method to one client instance.
 *
 * @param {import('../client.js').TidepoolClient} client
 */
export function buildResources(client) {
  return {
    activity: {
      list(options) {
        return client._call("get_v1_activity", options, undefined, undefined);
      },
      run(run, options) {
        return client._call("get_v1_activity_runs_run", options, run, undefined);
      },
    },
    apiKeys: {
      create(body, options) {
        return client._call("post_v1_api_keys", options, undefined, body);
      },
      list(options) {
        return client._call("get_v1_api_keys", options, undefined, undefined);
      },
      remove(id, options) {
        return client._call("delete_v1_api_keys_id", options, id, undefined);
      },
      whoami(options) {
        return client._call("get_v1_api_keys_whoami", options, undefined, undefined);
      },
    },
    chatrooms: {
      list(options) {
        return client._call("get_v1_chatrooms", options, undefined, undefined);
      },
      members(room, options) {
        return client._call("get_v1_chatrooms_room_members", options, room, undefined);
      },
      messages(room, options) {
        return client._call("get_v1_chatrooms_room_messages", options, room, undefined);
      },
      send(room, body, options) {
        return client._call("post_v1_chatrooms_room_messages", options, room, body);
      },
      stream(room, options) {
        return client._stream("get_v1_chatrooms_room_stream", options, room, undefined);
      },
    },
    connections: {
      create(body, options) {
        return client._call("post_v1_connections", options, undefined, body);
      },
      get(id, options) {
        return client._call("get_v1_connections_id", options, id, undefined);
      },
      list(options) {
        return client._call("get_v1_connections", options, undefined, undefined);
      },
      remove(id, options) {
        return client._call("delete_v1_connections_id", options, id, undefined);
      },
      update(id, body, options) {
        return client._call("patch_v1_connections_id", options, id, body);
      },
    },
    game: {
      auth: {
        login(body, options) {
          return client._call("post_v1_auth_login", options, undefined, body);
        },
        register(body, options) {
          return client._call("post_v1_auth_register", options, undefined, body);
        },
      },
      events: {
        create(body, options) {
          return client._call("post_v1_events", options, undefined, body);
        },
      },
      feedback: {
        create(body, options) {
          return client._call("post_v1_feedback", options, undefined, body);
        },
      },
      games: {
        settings(game_id, options) {
          return client._call("get_v1_games_game_id_settings", options, game_id, undefined);
        },
        updateSettings(game_id, body, options) {
          return client._call("put_v1_games_game_id_settings", options, game_id, body);
        },
      },
      leaderboards: {
        get(game_id, options) {
          return client._call("get_v1_leaderboards_game_id", options, game_id, undefined);
        },
        putEntry(params, body, options) {
          return client._call("put_v1_leaderboards_game_id_entries_player_id", options, params, body);
        },
        submitEntry(game_id, body, options) {
          return client._call("post_v1_leaderboards_game_id_entries", options, game_id, body);
        },
      },
      players: {
        createSubaccount(body, options) {
          return client._call("post_v1_players_subaccount", options, undefined, body);
        },
        get(id, options) {
          return client._call("get_v1_players_id", options, id, undefined);
        },
        remove(id, options) {
          return client._call("delete_v1_players_id", options, id, undefined);
        },
        subaccounts(id, options) {
          return client._call("get_v1_players_id_subaccounts", options, id, undefined);
        },
        update(id, body, options) {
          return client._call("put_v1_players_id", options, id, body);
        },
      },
      saves: {
        create(body, options) {
          return client._call("post_v1_saves", options, undefined, body);
        },
        get(params, options) {
          return client._call("get_v1_saves_player_id_save_id", options, params, undefined);
        },
        list(player_id, options) {
          return client._call("get_v1_saves_player_id", options, player_id, undefined);
        },
        remove(params, options) {
          return client._call("delete_v1_saves_player_id_save_id", options, params, undefined);
        },
      },
      socketTickets: {
        create(body, options) {
          return client._call("post_v1_socket_tickets", options, undefined, body);
        },
      },
    },
    hosts: {
      capabilities(id, options) {
        return client._call("get_v1_hosts_id_capabilities", options, id, undefined);
      },
      create(body, options) {
        return client._call("post_v1_hosts", options, undefined, body);
      },
      createExecution(id, body, options) {
        return client._call("post_v1_hosts_id_executions", options, id, body);
      },
      enroll(body, options) {
        return client._call("post_v1_hosts_enroll", options, undefined, body);
      },
      get(id, options) {
        return client._call("get_v1_hosts_id", options, id, undefined);
      },
      list(options) {
        return client._call("get_v1_hosts", options, undefined, undefined);
      },
      mintEnrollmentToken(body, options) {
        return client._call("post_v1_hosts_enrollment_tokens", options, undefined, body);
      },
      remove(id, options) {
        return client._call("delete_v1_hosts_id", options, id, undefined);
      },
      rename(id, body, options) {
        return client._call("patch_v1_hosts_id", options, id, body);
      },
      reportCapabilities(id, body, options) {
        return client._call("post_v1_hosts_id_capabilities", options, id, body);
      },
      startExecution(id, body, options) {
        return client._call("post_v1_hosts_id_execution", options, id, body);
      },
      stopExecution(id, options) {
        return client._call("delete_v1_hosts_id_execution", options, id, undefined);
      },
    },
    pool: {
      acceptInvitation(body, options) {
        return client._call("post_v1_pool_invitations_accept", options, undefined, body);
      },
      changePassword(body, options) {
        return client._call("post_v1_pool_password", options, undefined, body);
      },
      invitations(options) {
        return client._call("get_v1_pool_invitations", options, undefined, undefined);
      },
      invite(body, options) {
        return client._call("post_v1_pool_invitations", options, undefined, body);
      },
      login(body, options) {
        return client._call("post_v1_pool_login", options, undefined, body);
      },
      logout(body, options) {
        return client._call("post_v1_pool_logout", options, undefined, body);
      },
      me(options) {
        return client._call("get_v1_pool_me", options, undefined, undefined);
      },
      members(options) {
        return client._call("get_v1_pool_members", options, undefined, undefined);
      },
      removeMember(id, options) {
        return client._call("delete_v1_pool_members_id", options, id, undefined);
      },
      revokeInvitation(id, options) {
        return client._call("delete_v1_pool_invitations_id", options, id, undefined);
      },
      updateMember(id, body, options) {
        return client._call("patch_v1_pool_members_id", options, id, body);
      },
    },
    projects: {
      configuration(id, options) {
        return client._call("get_v1_projects_id_configuration", options, id, undefined);
      },
      create(body, options) {
        return client._call("post_v1_projects", options, undefined, body);
      },
      get(id, options) {
        return client._call("get_v1_projects_id", options, id, undefined);
      },
      installConfiguration(id, body, options) {
        return client._call("post_v1_projects_id_configuration_install", options, id, body);
      },
      list(options) {
        return client._call("get_v1_projects", options, undefined, undefined);
      },
      remove(id, options) {
        return client._call("delete_v1_projects_id", options, id, undefined);
      },
      rename(id, body, options) {
        return client._call("patch_v1_projects_id", options, id, body);
      },
      revision(params, options) {
        return client._call("get_v1_projects_id_revisions_revision_id", options, params, undefined);
      },
      revisions(id, options) {
        return client._call("get_v1_projects_id_revisions", options, id, undefined);
      },
      validateConfiguration(id, body, options) {
        return client._call("post_v1_projects_id_configuration_validate", options, id, body);
      },
    },
    relay: {
      enable(session, body, options) {
        return client._call("put_v1_sessions_session_relay", options, session, body);
      },
      events(session, options) {
        return client._call("get_v1_sessions_session_relay_events", options, session, undefined);
      },
      get(session, options) {
        return client._call("get_v1_sessions_session_relay", options, session, undefined);
      },
      publish(session, body, options) {
        return client._call("post_v1_sessions_session_relay_events", options, session, body);
      },
      stop(session, options) {
        return client._call("delete_v1_sessions_session_relay", options, session, undefined);
      },
    },
    runs: {
      dispatch(body, options) {
        return client._call("post_v1_runs", options, undefined, body);
      },
    },
    setup: {
      create(body, options) {
        return client._call("post_v1_setup", options, undefined, body);
      },
      status(options) {
        return client._call("get_v1_setup_status", options, undefined, undefined);
      },
    },
    system: {
      health(options) {
        return client._call("get_healthz", options, undefined, undefined);
      },
      metrics(options) {
        return client._call("get_metrics", options, undefined, undefined);
      },
      openapi(options) {
        return client._call("get_openapi_json", options, undefined, undefined);
      },
      openapiV1(options) {
        return client._call("get_openapi_v1_json", options, undefined, undefined);
      },
    },
    triggers: {
      install(body, options) {
        return client._call("post_v1_triggers_install", options, undefined, body);
      },
      validate(body, options) {
        return client._call("post_v1_triggers_validate", options, undefined, body);
      },
    },
    webhooks: {
      github(slug, body, options) {
        return client._call("post_v1_webhooks_github_slug", options, slug, body);
      },
      slack(slug, body, options) {
        return client._call("post_v1_webhooks_slack_slug", options, slug, body);
      },
    },
  };
}
