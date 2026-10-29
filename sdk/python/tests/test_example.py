"""The short example: enrol a host, then read activity."""

from __future__ import annotations

import support
from examples.enroll_and_activity import main, run


def host(status="pending_approval", host_id="host_1"):
    body = support.sample_schema(support.SCHEMAS["HostView"])
    body.update(
        {
            "id": host_id,
            "slug": "build-box",
            "display_name": "Build box",
            "hostname": "build-box.local",
            "status": status,
            "execution_enabled": True,
        }
    )
    return body


def minted():
    body = support.sample_schema(support.SCHEMAS["MintEnrollmentTokenResponse"])
    body["token"] = "enrol-token"
    body["enrollment_token"]["id"] = "enrol_1"
    return body


def activity_page():
    body = support.sample_schema(support.SCHEMAS["ActivityPage"])
    body["items"] = [support.sample_schema(support.SCHEMAS["ActivityEntry"])]
    body["items"][0]["id"] = "act_1"
    body["hasMore"] = False
    body["nextCursor"] = None
    return body


def responder(request):
    if request.path == "/v1/hosts" and request.method == "POST":
        assert request.json == {"display_name": "Build box", "hostname": "build-box.local"}
        return support.json_response(201, host())
    if request.path == "/v1/hosts/enrollment-tokens":
        assert request.json == {"host_id": "host_1"}
        return support.json_response(201, minted())
    if request.path == "/v1/hosts/enroll":
        assert request.json == {
            "token": "enrol-token",
            "credential": "host-generated-secret",
        }
        return support.json_response(201, host(status="connected"))
    if request.path == "/v1/activity":
        return support.json_response(200, activity_page())
    return support.problem_response(404, "not_found")


def test_example_enrols_a_host_and_reads_activity():
    with support.RecordedServer(responder) as server:
        result = run(server.base_url, "operator-token")
        assert result.host.id == "host_1"
        assert result.enrolled.status == "connected"
        assert result.activity.items[0].id == "act_1"
        assert [request.path for request in server.requests] == [
            "/v1/hosts",
            "/v1/hosts/enrollment-tokens",
            "/v1/hosts/enroll",
            "/v1/activity",
        ]


def test_example_command_line_prints_no_token(capsys):
    with support.RecordedServer(responder) as server:
        code = main(["--base-url", server.base_url, "--token", "operator-token"])
    output = capsys.readouterr().out
    assert code == 0
    assert "enrolled build-box (connected)" in output
    assert "activity entries on page one: 1" in output
    assert "operator-token" not in output
