"""Base class for the generated contract models.

A model is a plain dataclass whose fields the generator derives from a schema.
Parsing is deliberately tolerant, so a response that gained an unknown member
or lost an optional one still becomes a usable object:

* an unknown member is preserved in ``to_dict()`` rather than dropped;
* a member missing from the body stays ``None``;
* an open-enum value is kept verbatim in its ``str`` field.

``to_dict()`` emits exactly the members the body carried (plus every non-None
value a caller set), so ``Model.from_dict(body).to_dict() == body`` holds for a
contract-shaped body.
"""

from __future__ import annotations

import dataclasses
import types
import typing
from functools import lru_cache
from typing import Any, Dict, Mapping, Type, TypeVar, Union, get_args, get_origin

T = TypeVar("T", bound="Model")

_NONE_TYPE = type(None)
_UNION_TYPE = getattr(types, "UnionType", None)


class Model:
    """Base class for every generated model.

    Subclasses are dataclasses; this class supplies :meth:`from_dict` and
    :meth:`to_dict`, and records which JSON members were present so an absent
    optional member and an explicit ``null`` stay distinguishable.
    """

    __json_names__: Mapping[str, str] = {}

    @classmethod
    def from_dict(cls: Type[T], data: Mapping[str, Any]) -> T:
        """Build a model from one JSON object.

        Args:
            data: A decoded JSON object.

        Returns:
            The typed model. Unknown members are preserved for ``to_dict()``.
        """
        if not isinstance(data, Mapping):
            raise TypeError(f"{cls.__name__}.from_dict expected a mapping")
        hints = _hints(cls)
        instance = cls()
        present: set[str] = set()
        known = cls.json_names()
        for item in dataclasses.fields(cls):
            json_name = known.get(item.name, item.name)
            if json_name in data:
                value = _parse_value(data[json_name], hints.get(item.name, Any))
                setattr(instance, item.name, value)
                present.add(item.name)
        known_json = set(known.values()) | {
            item.name for item in dataclasses.fields(cls) if item.name not in known
        }
        instance._extra = {key: value for key, value in data.items() if key not in known_json}
        instance._present = frozenset(present)
        return instance

    def to_dict(self) -> Dict[str, Any]:
        """Render the model as the JSON object the contract describes.

        Returns:
            A dict containing the members this instance carries.
        """
        known = self.__class__.json_names()
        present = getattr(self, "_present", frozenset())
        result: Dict[str, Any] = {}
        for item in dataclasses.fields(self):
            value = getattr(self, item.name, None)
            if value is None and item.name not in present:
                continue
            result[known.get(item.name, item.name)] = _dump_value(value)
        result.update(getattr(self, "_extra", {}))
        return result

    @classmethod
    def json_names(cls) -> Mapping[str, str]:
        """Map Python attribute names to JSON member names."""
        return dict(getattr(cls, "__json_names__", {}) or {})


class ModelList(list):
    """Base class for a generated top-level array schema.

    The contract has a handful of routes whose success body is a JSON array
    rather than an object; the generator emits a list subclass with the item
    model attached so parsing stays typed.
    """

    item_type: Any = Any

    @classmethod
    def from_dict(cls, data: Any) -> "ModelList":
        """Parse a JSON array into a typed model list."""
        if not isinstance(data, list):
            raise TypeError(f"{cls.__name__}.from_dict expected a list")
        item_type = getattr(cls, "item_type", Any)
        return cls(_parse_value(item, item_type) for item in data)

    def to_dict(self) -> list:
        """Render the list as JSON-compatible data."""
        return [_dump_value(item) for item in self]


@lru_cache(maxsize=None)
def _hints(cls: Type[Model]) -> Mapping[str, Any]:
    """Resolved annotations for a model class."""
    return typing.get_type_hints(cls)


def _parse_value(value: Any, hint: Any) -> Any:
    """Parse one JSON value against a resolved annotation."""
    if value is None:
        return None
    origin = get_origin(hint)
    if origin is Union or (_UNION_TYPE is not None and origin is _UNION_TYPE):
        for candidate in get_args(hint):
            if candidate is _NONE_TYPE:
                continue
            return _parse_value(value, candidate)
        return value
    if origin in (list, typing.List):
        arguments = get_args(hint)
        item_hint = arguments[0] if arguments else Any
        return [_parse_value(item, item_hint) for item in value]
    if origin in (dict, typing.Dict):
        arguments = get_args(hint)
        item_hint = arguments[1] if len(arguments) > 1 else Any
        return {key: _parse_value(item, item_hint) for key, item in value.items()}
    if isinstance(hint, type) and issubclass(hint, ModelList):
        return hint.from_dict(value)
    if isinstance(hint, type) and issubclass(hint, Model):
        return hint.from_dict(value)
    return value


def _dump_value(value: Any) -> Any:
    """Render one model value as JSON-compatible data."""
    if isinstance(value, Model):
        return value.to_dict()
    if isinstance(value, list):
        return [_dump_value(item) for item in value]
    if isinstance(value, dict):
        return {key: _dump_value(item) for key, item in value.items()}
    return value
