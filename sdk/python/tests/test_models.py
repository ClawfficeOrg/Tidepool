"""Typed model round-trips against every contract schema."""

from __future__ import annotations

import pytest

import support
from tidepool_client._generated import models as generated
from tidepool_client._generated.models import ActivityPage, HostView, Problem, RunView


PROPERTY_SCHEMAS = sorted(
    name for name, schema in support.SCHEMAS.items() if schema.get("properties")
)


@pytest.mark.parametrize("name", PROPERTY_SCHEMAS)
def test_model_round_trips_a_contract_shaped_body(name):
    schema = support.SCHEMAS[name]
    sample = support.sample_schema(schema)
    model = getattr(generated, name)
    parsed = model.from_dict(sample)
    assert parsed.to_dict() == sample


def test_nested_models_are_typed():
    sample = support.sample_schema(support.SCHEMAS["RunView"])
    run = RunView.from_dict(sample)
    assert run.steps
    assert all(isinstance(step, generated.RunStepView) for step in run.steps)
    assert run.to_dict()["steps"][0]["id"] == sample["steps"][0]["id"]


def test_unknown_response_fields_are_preserved():
    body = support.sample_schema(support.SCHEMAS["HostView"])
    body["future_member"] = {"added": "later"}
    host = HostView.from_dict(body)
    assert host.to_dict()["future_member"] == {"added": "later"}
    assert host.to_dict() == body


def test_unknown_open_enum_values_are_opaque_strings():
    body = support.sample_schema(support.SCHEMAS["HostView"])
    body["status"] = "quantum_connected"
    host = HostView.from_dict(body)
    assert host.status == "quantum_connected"
    assert host.to_dict()["status"] == "quantum_connected"


def test_absent_optional_members_stay_absent_on_the_way_back():
    host = HostView.from_dict({"id": "host_1"})
    assert host.id == "host_1"
    assert host.display_name is None
    assert host.to_dict() == {"id": "host_1"}


def test_explicit_null_is_distinguishable_from_absence():
    host = HostView.from_dict({"id": "host_1", "revoked_at": None})
    assert host.to_dict() == {"id": "host_1", "revoked_at": None}


def test_problem_renames_the_type_member_and_keeps_unknown_codes():
    problem = Problem.from_dict(
        {
            "type": "https://tidepool.test/problems",
            "title": "Gone",
            "status": 410,
            "code": "a_future_code",
            "detail": "gone",
            "requestId": "req_1",
        }
    )
    assert problem.type_ == "https://tidepool.test/problems"
    assert problem.code == "a_future_code"
    assert problem.to_dict()["type"] == "https://tidepool.test/problems"


def test_partial_bodies_do_not_raise():
    activity = ActivityPage.from_dict({"items": []})
    assert activity.items == []
    assert activity.has_more is None


def test_parsed_models_compare_by_value():
    sample = support.sample_schema(support.SCHEMAS["ActivityPage"])
    assert ActivityPage.from_dict(sample) == ActivityPage.from_dict(sample)


def test_from_dict_rejects_a_non_mapping():
    with pytest.raises(TypeError):
        HostView.from_dict(["not", "an", "object"])


def test_top_level_array_models_parse_and_round_trip():
    body = [support.sample_schema(support.SCHEMAS["RoomView"])]
    rooms = generated.RoomViewList.from_dict(body)
    assert isinstance(rooms, generated.RoomViewList)
    assert isinstance(rooms[0], generated.RoomView)
    assert rooms.to_dict() == body
