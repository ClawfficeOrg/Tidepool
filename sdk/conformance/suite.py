"""Cross-SDK conformance suite core.

One scenario list, three bindings. Every scenario is a JSON document under
``sdk/conformance/scenarios/``; the runner sends each step to every binding's
adapter over a line-oriented JSON protocol and compares what came back.

The suite deliberately knows nothing about any one SDK. A binding is a process
that implements the commands in :data:`COMMANDS`; everything above that is
shared. That is what makes a new scenario a data change rather than three
tests: add a step to the scenario list and every binding runs it.

Comparison happens twice:

* against the scenario's ``expect`` pattern, which pins the behaviour the
  contract promises (a problem code, a paging cursor, a status);
* across bindings for the paths a step lists in ``agree``, which catches a
  binding that answers differently from its siblings even when both answers
  were not pinned in advance.

Failure messages never include captured variables, credentials or request
parameters, and response values are redacted before printing.
"""

from __future__ import annotations

import json
import os
import queue
import re
import subprocess
import threading
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Dict, Iterable, List, Mapping, Optional, Sequence, Tuple

CONFORMANCE_DIR = Path(__file__).resolve().parent
REPOSITORY = CONFORMANCE_DIR.parents[1]
DEFAULT_SCENARIO_DIR = CONFORMANCE_DIR / "scenarios"
DEFAULT_MANIFEST = CONFORMANCE_DIR / "adapters" / "manifest.json"

#: Every command an adapter must implement. Adding a command is an adapter
#: change by design; adding a *scenario* is not.
COMMANDS: Tuple[str, ...] = (
    "contract",
    "describe",
    "call",
    "page",
    "stream",
    "frames",
)

#: Walking a page past this many continuations means a cursor never ended.
MAX_PAGES = 100

MISSING = object()

_TEMPLATE = re.compile(r"\{\{([A-Za-z0-9_]+)\}\}")
_SENSITIVE = re.compile(
    r"(token|secret|credential|password|api[-_]?key|authorization|bearer)",
    re.IGNORECASE,
)


class SuiteError(Exception):
    """The suite cannot run as configured: a bad manifest, scenario or var."""


# ── values, paths and redaction ────────────────────────────────────────────


def resolve_path(value: Any, path: str) -> Any:
    """Resolve a dotted path (``body.items.0.id``) or return :data:`MISSING`."""
    current = value
    for part in path.split("."):
        if isinstance(current, Mapping):
            if part not in current:
                return MISSING
            current = current[part]
            continue
        if isinstance(current, list):
            try:
                index = int(part)
            except ValueError:
                return MISSING
            if index < 0 or index >= len(current):
                return MISSING
            current = current[index]
            continue
        return MISSING
    return current


def redact(value: Any) -> Any:
    """Return a copy safe to print: credential-shaped values are blanked."""
    if isinstance(value, Mapping):
        result: Dict[str, Any] = {}
        for key, item in value.items():
            if isinstance(key, str) and _SENSITIVE.search(key):
                result[key] = "<redacted>"
            else:
                result[key] = redact(item)
        return result
    if isinstance(value, list):
        return [redact(item) for item in value]
    return value


def describe_value(value: Any, limit: int = 320) -> str:
    """Render one value for a failure message, redacted and truncated."""
    if value is MISSING:
        return "<missing>"
    text = json.dumps(redact(value), ensure_ascii=False, sort_keys=True, default=str)
    if len(text) > limit:
        return text[:limit] + "…"
    return text


def interpolate(value: Any, variables: Mapping[str, Any], where: str) -> Any:
    """Replace ``{{name}}`` templates in a string, list or object."""
    if isinstance(value, str):
        def substitute(match: re.Match[str]) -> str:
            name = match.group(1)
            if name not in variables:
                raise SuiteError(
                    f"{where}: no variable named {name!r}; it is neither provided "
                    "nor captured by an earlier step"
                )
            replacement = variables[name]
            if isinstance(replacement, bool):
                return "true" if replacement else "false"
            return str(replacement)

        return _TEMPLATE.sub(substitute, value)
    if isinstance(value, list):
        return [interpolate(item, variables, where) for item in value]
    if isinstance(value, Mapping):
        return {
            key: interpolate(item, variables, where) for key, item in value.items()
        }
    return value


def template_names(value: Any) -> Iterable[str]:
    """Every ``{{name}}`` referenced by a value."""
    if isinstance(value, str):
        for match in _TEMPLATE.finditer(value):
            yield match.group(1)
    elif isinstance(value, list):
        for item in value:
            yield from template_names(item)
    elif isinstance(value, Mapping):
        for item in value.values():
            yield from template_names(item)


# ── expectation matching ───────────────────────────────────────────────────


@dataclass(frozen=True)
class Mismatch:
    """One expectation or agreement that did not hold."""

    path: str
    message: str


def _is_operator(pattern: Mapping[str, Any]) -> bool:
    return bool(pattern) and all(
        isinstance(key, str) and key.startswith("$") for key in pattern
    )


def _type_name(value: Any) -> str:
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "boolean"
    if isinstance(value, int):
        return "integer"
    if isinstance(value, float):
        return "number"
    if isinstance(value, str):
        return "string"
    if isinstance(value, list):
        return "array"
    if isinstance(value, Mapping):
        return "object"
    return type(value).__name__


def _is_number(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def _match_operator(
    operator: str, operand: Any, actual: Any, path: str
) -> List[Mismatch]:
    shown = describe_value(actual)
    if operator == "$eq":
        return [] if actual == operand else [
            Mismatch(path, f"expected {describe_value(operand)}, got {shown}")
        ]
    if operator == "$ne":
        return [] if actual != operand else [
            Mismatch(path, f"expected anything but {describe_value(operand)}")
        ]
    if operator in ("$gt", "$gte", "$lt", "$lte"):
        if not _is_number(actual) or not _is_number(operand):
            return [Mismatch(path, f"{operator} needs numbers, got {shown}")]
        passes = {
            "$gt": actual > operand,
            "$gte": actual >= operand,
            "$lt": actual < operand,
            "$lte": actual <= operand,
        }[operator]
        return [] if passes else [
            Mismatch(path, f"expected {operator} {operand}, got {shown}")
        ]
    if operator == "$type":
        actual_type = _type_name(actual)
        wanted = str(operand)
        if wanted == "number" and actual_type == "integer":
            return []
        return [] if actual_type == wanted else [
            Mismatch(path, f"expected type {wanted}, got {actual_type}")
        ]
    if operator == "$len":
        if not isinstance(actual, (str, list, Mapping)):
            return [Mismatch(path, f"$len needs a sized value, got {shown}")]
        length = len(actual)
        if isinstance(operand, Mapping):
            return _match_pattern(operand, length, path + ".$len")
        return [] if length == operand else [
            Mismatch(path, f"expected length {operand}, got {length}")
        ]
    if operator == "$contains":
        if isinstance(actual, str):
            return [] if isinstance(operand, str) and operand in actual else [
                Mismatch(path, f"expected to contain {describe_value(operand)}")
            ]
        if isinstance(actual, list):
            for item in actual:
                if isinstance(operand, Mapping) and isinstance(item, Mapping):
                    if not _match_pattern(operand, item, path):
                        return []
                elif item == operand:
                    return []
            return [Mismatch(path, f"expected a member matching {describe_value(operand)}")]
        if isinstance(actual, Mapping):
            return [] if operand in actual else [
                Mismatch(path, f"expected key {describe_value(operand)}")
            ]
        return [Mismatch(path, f"$contains needs a sized value, got {shown}")]
    if operator == "$not":
        return [] if _match_pattern(operand, actual, path) else [
            Mismatch(path, f"expected not {describe_value(operand)}, got {shown}")
        ]
    if operator == "$exists":
        exists = actual is not MISSING
        return [] if exists == bool(operand) else [
            Mismatch(path, f"expected exists={bool(operand)}, got exists={exists}")
        ]
    if operator == "$regex":
        if not isinstance(actual, str) or not isinstance(operand, str):
            return [Mismatch(path, f"$regex needs strings, got {shown}")]
        return [] if re.search(operand, actual) else [
            Mismatch(path, f"expected to match {operand!r}, got {shown}")
        ]
    if operator == "$oneof":
        if not isinstance(operand, list):
            return [Mismatch(path, "$oneof needs a list")]
        return [] if actual in operand else [
            Mismatch(path, f"expected one of {describe_value(operand)}, got {shown}")
        ]
    raise SuiteError(f"unknown expectation operator {operator!r} at {path}")


def _match_pattern(pattern: Any, actual: Any, path: str) -> List[Mismatch]:
    if isinstance(pattern, Mapping):
        if _is_operator(pattern):
            mismatches: List[Mismatch] = []
            for operator, operand in pattern.items():
                mismatches.extend(_match_operator(operator, operand, actual, path))
            return mismatches
        if not isinstance(actual, Mapping):
            return [
                Mismatch(path, f"expected an object, got {describe_value(actual)}")
            ]
        mismatches = []
        for key, value in pattern.items():
            child = f"{path}.{key}" if path else key
            if key in actual:
                mismatches.extend(_match_pattern(value, actual[key], child))
                continue
            # A dotted key is a path into the observed value, so a scenario can
            # pin ``body.run.id`` without nesting the whole response.
            if "." in key:
                resolved = resolve_path(actual, key)
                if resolved is MISSING:
                    mismatches.append(
                        Mismatch(child, f"missing (expected {describe_value(value)})")
                    )
                    continue
                mismatches.extend(_match_pattern(value, resolved, child))
                continue
            mismatches.append(Mismatch(child, f"missing (expected {describe_value(value)})"))
        return mismatches
    if isinstance(pattern, list):
        if not isinstance(actual, list):
            return [Mismatch(path, f"expected an array, got {describe_value(actual)}")]
        if len(pattern) != len(actual):
            return [
                Mismatch(
                    path,
                    f"expected {len(pattern)} items, got {len(actual)}",
                )
            ]
        mismatches = []
        for index, (wanted, got) in enumerate(zip(pattern, actual)):
            mismatches.extend(_match_pattern(wanted, got, f"{path}.{index}".strip(".")))
        return mismatches
    if isinstance(pattern, bool) or isinstance(actual, bool):
        if pattern is actual or (type(pattern) is bool and type(actual) is bool and pattern == actual):
            return []
        return [
            Mismatch(path, f"expected {describe_value(pattern)}, got {describe_value(actual)}")
        ]
    if pattern != actual:
        return [
            Mismatch(path, f"expected {describe_value(pattern)}, got {describe_value(actual)}")
        ]
    return []


def match_expectation(pattern: Any, actual: Any) -> List[Mismatch]:
    """Compare one expectation pattern with one observation."""
    if pattern is None:
        return []
    return _match_pattern(pattern, actual, "")


# ── scenario model ─────────────────────────────────────────────────────────


@dataclass
class Step:
    """One command sent to every binding."""

    id: str
    command: str
    params: Dict[str, Any] = field(default_factory=dict)
    auth: Dict[str, Any] = field(default_factory=dict)
    expect: Any = None
    agree: Tuple[str, ...] = ()
    capture: Dict[str, str] = field(default_factory=dict)
    note: str = ""


@dataclass
class Scenario:
    """A named list of steps every binding must pass."""

    id: str
    title: str
    goal: str
    steps: List[Step]
    source: str


@dataclass(frozen=True)
class Binding:
    """One SDK under test, reached through its adapter process."""

    id: str
    language: str
    command: Tuple[str, ...]


def load_manifest(path: Path = DEFAULT_MANIFEST) -> List[Binding]:
    """Read the adapter manifest; every binding listed is required."""
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except OSError as error:
        raise SuiteError(f"cannot read adapter manifest {path}: {error}") from error
    except ValueError as error:
        raise SuiteError(f"adapter manifest {path} is not JSON: {error}") from error
    if document.get("protocol") != 1:
        raise SuiteError(f"{path}: unsupported adapter protocol {document.get('protocol')!r}")
    bindings = []
    seen = set()
    for entry in document.get("bindings", []):
        binding_id = entry.get("id")
        command = entry.get("command")
        if not isinstance(binding_id, str) or not binding_id:
            raise SuiteError(f"{path}: every binding needs a non-empty id")
        if binding_id in seen:
            raise SuiteError(f"{path}: duplicate binding id {binding_id!r}")
        seen.add(binding_id)
        if not isinstance(command, list) or not command or not all(
            isinstance(part, str) and part for part in command
        ):
            raise SuiteError(f"{path}: binding {binding_id!r} needs a command array")
        bindings.append(
            Binding(
                id=binding_id,
                language=str(entry.get("language", binding_id)),
                command=tuple(command),
            )
        )
    if not bindings:
        raise SuiteError(f"{path}: no bindings listed")
    return bindings


def load_scenarios(
    directory: Path = DEFAULT_SCENARIO_DIR,
    variables: Optional[Mapping[str, Any]] = None,
) -> List[Scenario]:
    """Read every scenario document under ``directory`` in sorted file order."""
    base_variables = dict(variables or {})
    scenarios: List[Scenario] = []
    seen_scenarios = set()
    paths = sorted(directory.glob("*.json"))
    if not paths:
        raise SuiteError(f"no scenario files under {directory}")
    for path in paths:
        try:
            document = json.loads(path.read_text(encoding="utf-8"))
        except OSError as error:
            raise SuiteError(f"cannot read scenario file {path}: {error}") from error
        except ValueError as error:
            raise SuiteError(f"scenario file {path} is not JSON: {error}") from error
        entries = document.get("scenarios")
        if not isinstance(entries, list) or not entries:
            raise SuiteError(f"{path}: expected a non-empty 'scenarios' array")
        for entry in entries:
            scenario_id = entry.get("id")
            if not isinstance(scenario_id, str) or not scenario_id:
                raise SuiteError(f"{path}: every scenario needs a non-empty id")
            if scenario_id in seen_scenarios:
                raise SuiteError(f"{path}: duplicate scenario id {scenario_id!r}")
            seen_scenarios.add(scenario_id)
            steps = _load_steps(path, scenario_id, entry.get("steps"), base_variables)
            scenarios.append(
                Scenario(
                    id=scenario_id,
                    title=str(entry.get("title", scenario_id)),
                    goal=str(entry.get("goal", "")),
                    steps=steps,
                    source=path.name,
                )
            )
    return scenarios


def _load_steps(
    path: Path,
    scenario_id: str,
    raw_steps: Any,
    variables: Mapping[str, Any],
) -> List[Step]:
    if not isinstance(raw_steps, list) or not raw_steps:
        raise SuiteError(f"{path}: scenario {scenario_id!r} needs a non-empty steps array")
    known = set(variables)
    # ``binding`` is supplied by the runner for every step; a scenario may use
    # it to give each binding its own name (hostnames, delivery keys).
    known.add("binding")
    steps: List[Step] = []
    seen_steps = set()
    for index, raw in enumerate(raw_steps, start=1):
        if not isinstance(raw, dict):
            raise SuiteError(f"{path}: scenario {scenario_id!r} step {index} is not an object")
        step_id = str(raw.get("id") or f"step-{index}")
        if step_id in seen_steps:
            raise SuiteError(f"{path}: scenario {scenario_id!r} duplicates step {step_id!r}")
        seen_steps.add(step_id)
        command = raw.get("command")
        if command not in COMMANDS:
            raise SuiteError(
                f"{path}: scenario {scenario_id!r} step {step_id!r} uses unknown command "
                f"{command!r}; an adapter must implement it first (known: {', '.join(COMMANDS)})"
            )
        params = raw.get("params", {}) or {}
        auth = raw.get("auth", {}) or {}
        if not isinstance(params, dict) or not isinstance(auth, dict):
            raise SuiteError(
                f"{path}: scenario {scenario_id!r} step {step_id!r}: params and auth must be objects"
            )
        capture = raw.get("capture", {}) or {}
        if not isinstance(capture, dict) or not all(
            isinstance(key, str) and isinstance(value, str)
            for key, value in capture.items()
        ):
            raise SuiteError(
                f"{path}: scenario {scenario_id!r} step {step_id!r}: capture must map names to paths"
            )
        agree = raw.get("agree", []) or []
        if not isinstance(agree, list) or not all(isinstance(item, str) for item in agree):
            raise SuiteError(
                f"{path}: scenario {scenario_id!r} step {step_id!r}: agree must be a string array"
            )
        where = f"scenario {scenario_id!r} step {step_id!r}"
        for name in tuple(template_names(params)) + tuple(template_names(auth)):
            if name not in known:
                raise SuiteError(
                    f"{where}: references variable {name!r}, which is neither provided "
                    "nor captured before this step"
                )
        steps.append(
            Step(
                id=step_id,
                command=command,
                params=dict(params),
                auth=dict(auth),
                expect=raw.get("expect"),
                agree=tuple(agree),
                capture=dict(capture),
                note=str(raw.get("note", "")),
            )
        )
        known.update(capture)
    return steps


# ── adapter processes ──────────────────────────────────────────────────────


class AdapterError(Exception):
    """An adapter failed to answer, crashed, or answered out of protocol."""


class Adapter:
    """One binding's adapter process, spoken to over newline-delimited JSON.

    Requests carry an ``id``, a ``command`` and ``params``. Responses carry the
    same ``id`` and either ``result`` or ``error``. Anything the process writes
    to stdout that is not a response is a protocol failure, so an adapter
    cannot print diagnostics into the suite's stream.
    """

    def __init__(
        self,
        binding: Binding,
        base_url: str,
        environment: Optional[Mapping[str, str]] = None,
        timeout: float = 180.0,
    ) -> None:
        self.binding = binding
        self.base_url = base_url
        self.timeout = timeout
        self._next_id = 0
        self._responses: "queue.Queue[Optional[str]]" = queue.Queue()
        self._stderr: List[str] = []
        self._stderr_lock = threading.Lock()
        self._process: Optional[subprocess.Popen[str]] = None
        env = dict(os.environ)
        env.update(environment or {})
        env["TIDEPOOL_CONFORMANCE_BASE_URL"] = base_url
        env["PYTHONUNBUFFERED"] = "1"
        command = list(binding.command)
        if command and not os.path.isabs(command[0]) and os.sep in command[0]:
            command[0] = str((REPOSITORY / command[0]).resolve())
        self._command = command
        self._env = env

    def start(self) -> None:
        """Launch the adapter and start draining its pipes."""
        if self._process is not None:
            return
        try:
            self._process = subprocess.Popen(
                self._command,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                bufsize=1,
                env=self._env,
            )
        except OSError as error:
            raise AdapterError(
                f"{self.binding.id}: cannot start {self._command[0]!r}: {error}"
            ) from error
        threading.Thread(target=self._read_stdout, daemon=True).start()
        threading.Thread(target=self._read_stderr, daemon=True).start()

    def _read_stdout(self) -> None:
        process = self._process
        assert process is not None and process.stdout is not None
        for line in process.stdout:
            self._responses.put(line)
        self._responses.put(None)

    def _read_stderr(self) -> None:
        process = self._process
        assert process is not None and process.stderr is not None
        for line in process.stderr:
            with self._stderr_lock:
                if len(self._stderr) < 40:
                    self._stderr.append(line.rstrip("\n"))

    def request(self, command: str, params: Mapping[str, Any]) -> Any:
        """Send one command and wait for its response."""
        if self._process is None or self._process.poll() is not None:
            raise AdapterError(f"{self.binding.id}: adapter is not running")
        self._next_id += 1
        request_id = self._next_id
        payload = json.dumps(
            {"id": request_id, "command": command, "params": params},
            ensure_ascii=False,
        )
        try:
            assert self._process.stdin is not None
            self._process.stdin.write(payload + "\n")
            self._process.stdin.flush()
        except (BrokenPipeError, OSError) as error:
            raise AdapterError(
                f"{self.binding.id}: adapter pipe closed ({error.__class__.__name__})"
            ) from error
        try:
            line = self._responses.get(timeout=self.timeout)
        except queue.Empty:
            raise AdapterError(
                f"{self.binding.id}: no response to {command} within {self.timeout:g}s"
            ) from None
        if line is None:
            raise AdapterError(
                f"{self.binding.id}: adapter exited during {command}{self._stderr_tail()}"
            )
        try:
            response = json.loads(line)
        except ValueError:
            raise AdapterError(
                f"{self.binding.id}: adapter wrote non-JSON to stdout during {command}"
            ) from None
        if not isinstance(response, dict) or response.get("id") != request_id:
            raise AdapterError(
                f"{self.binding.id}: adapter response during {command} is not the reply "
                "to the request it was sent"
            )
        if response.get("ok") is not True:
            error = response.get("error") or {}
            message = error.get("message") if isinstance(error, dict) else None
            kind = error.get("type") if isinstance(error, dict) else None
            label = kind or "error"
            raise AdapterError(
                f"{self.binding.id}: {command} failed in the adapter ({label}): "
                f"{message or 'no message'}"
            )
        return response.get("result")

    def _stderr_tail(self) -> str:
        with self._stderr_lock:
            lines = self._stderr[-5:]
        if not lines:
            return ""
        return "; stderr: " + " | ".join(redacted_line(line) for line in lines)

    def close(self) -> None:
        """Stop the adapter, politely first."""
        process = self._process
        if process is None:
            return
        try:
            if process.stdin is not None:
                process.stdin.close()
            process.wait(timeout=5)
        except (OSError, subprocess.TimeoutExpired):
            process.kill()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                pass
        finally:
            self._process = None


def redacted_line(line: str) -> str:
    """Redact credential-shaped assignments in an adapter's stderr line."""
    return re.sub(
        r"(?i)(token|secret|credential|password|api[-_]?key)\s*[=:]\s*\S+",
        r"\1=<redacted>",
        line,
    )[:200]


# ── results and the runner ─────────────────────────────────────────────────


@dataclass
class StepOutcome:
    """One binding's answer to one step, and why it passed or failed."""

    binding: str
    scenario: str
    step: str
    result: Any = None
    failures: List[str] = field(default_factory=list)
    skipped: Optional[str] = None
    captured: Tuple[str, ...] = ()

    @property
    def passed(self) -> bool:
        """True when the step ran, the adapter answered, and nothing mismatched."""
        return not self.failures and self.skipped is None


@dataclass
class Report:
    """The whole run: every scenario, step and binding, plus the verdict."""

    outcomes: List[StepOutcome] = field(default_factory=list)
    bindings: List[str] = field(default_factory=list)
    scenarios: List[str] = field(default_factory=list)
    missing_bindings: Tuple[str, ...] = ()
    base_url: str = ""
    variables_checked: int = 0

    @property
    def failures(self) -> List[StepOutcome]:
        """Every outcome that failed (skips are failures too)."""
        return [outcome for outcome in self.outcomes if not outcome.passed]

    @property
    def coverage_complete(self) -> bool:
        """True when every binding in the manifest ran every scenario."""
        return not self.missing_bindings

    @property
    def ok(self) -> bool:
        """True only for a clean pass with full coverage."""
        return self.coverage_complete and not self.failures

    def summary(self) -> Dict[str, Any]:
        """A JSON-safe summary; never includes credentials or variables."""
        return {
            "base_url": self.base_url,
            "bindings": list(self.bindings),
            "missing_bindings": list(self.missing_bindings),
            "scenarios": list(self.scenarios),
            "steps": len(self.outcomes),
            "passed": len(self.outcomes) - len(self.failures),
            "failed": len(self.failures),
            "ok": self.ok,
            "results": [
                {
                    "scenario": outcome.scenario,
                    "step": outcome.step,
                    "binding": outcome.binding,
                    "status": (
                        "skipped"
                        if outcome.skipped
                        else ("pass" if not outcome.failures else "fail")
                    ),
                    "captured": list(outcome.captured),
                    "failures": [
                        redacted_line(message) for message in outcome.failures
                    ],
                }
                for outcome in self.outcomes
            ],
        }


def run_suite(
    bindings: Sequence[Binding],
    scenarios: Sequence[Scenario],
    variables: Mapping[str, Any],
    base_url: str,
    *,
    selected: Optional[Sequence[str]] = None,
    timeout: float = 180.0,
    environment: Optional[Mapping[str, str]] = None,
) -> Report:
    """Run every scenario against every binding and return the verdict.

    ``selected`` narrows the run for debugging; the resulting report is marked
    as incomplete so a caller cannot mistake it for a clean pass.
    """
    selected_ids = list(selected) if selected else [binding.id for binding in bindings]
    unknown = sorted(set(selected_ids) - {binding.id for binding in bindings})
    if unknown:
        raise SuiteError("unknown binding(s): " + ", ".join(unknown))
    active = [binding for binding in bindings if binding.id in selected_ids]
    missing = tuple(
        binding.id for binding in bindings if binding.id not in selected_ids
    )
    report = Report(
        bindings=[binding.id for binding in active],
        scenarios=[scenario.id for scenario in scenarios],
        missing_bindings=missing,
        base_url=base_url,
    )

    adapters: Dict[str, Adapter] = {}
    dead: Dict[str, str] = {}
    for binding in active:
        adapters[binding.id] = Adapter(
            binding, base_url, environment=environment, timeout=timeout
        )
    try:
        for binding in active:
            try:
                adapters[binding.id].start()
            except AdapterError as error:
                dead[binding.id] = str(error)
        for scenario in scenarios:
            # Captures are per binding: two bindings registering their own host
            # must each keep their own id, or the second would consume the
            # first's one-time token and the run would fail for the wrong reason.
            scopes: Dict[str, Dict[str, Any]] = {
                binding.id: dict(variables) for binding in active
            }
            failed_bindings: Dict[str, str] = {}
            for step in scenario.steps:
                for binding in active:
                    if binding.id in dead:
                        report.outcomes.append(
                            StepOutcome(
                                binding=binding.id,
                                scenario=scenario.id,
                                step=step.id,
                                skipped=dead[binding.id],
                            )
                        )
                        continue
                    if binding.id in failed_bindings:
                        report.outcomes.append(
                            StepOutcome(
                                binding=binding.id,
                                scenario=scenario.id,
                                step=step.id,
                                skipped=(
                                    f"skipped after an earlier failure in {scenario.id}: "
                                    + failed_bindings[binding.id]
                                ),
                            )
                        )
                        continue
                    scope = dict(scopes[binding.id])
                    scope["binding"] = binding.id
                    outcome = _run_step(
                        adapters[binding.id], scenario.id, step, scope
                    )
                    report.outcomes.append(outcome)
                    if not outcome.passed:
                        failed_bindings[binding.id] = outcome.failures[0] if outcome.failures else "skipped"
                        continue
                    for name, value in _captures(step, outcome.result):
                        scopes[binding.id][name] = value
                if step.agree and not failed_bindings:
                    _check_agreement(report, scenario, step, active)
            # Captures from a failed scenario do not leak into the next one.
    finally:
        for adapter in adapters.values():
            adapter.close()
    return report


def _run_step(
    adapter: Adapter,
    scenario_id: str,
    step: Step,
    variables: Mapping[str, Any],
) -> StepOutcome:
    outcome = StepOutcome(
        binding=adapter.binding.id, scenario=scenario_id, step=step.id
    )
    params = interpolate(step.params, variables, f"step {step.id!r}")
    auth = interpolate(step.auth, variables, f"step {step.id!r} auth")
    request_params = dict(params)
    request_params["auth"] = auth
    try:
        result = adapter.request(step.command, request_params)
    except AdapterError as error:
        outcome.failures.append(str(error))
        return outcome
    outcome.result = result
    outcome.captured = tuple(step.capture)
    for name, path in step.capture.items():
        value = resolve_path(result, path)
        if value is MISSING:
            outcome.failures.append(
                f"capture {name!r}: {path!r} is not in the result "
                f"({describe_value(result)})"
            )
    if not outcome.failures:
        mismatches = match_expectation(step.expect, result)
        outcome.failures.extend(
            f"{mismatch.path or 'result'}: {mismatch.message}" for mismatch in mismatches
        )
    return outcome


def _captures(step: Step, result: Any) -> Iterable[Tuple[str, Any]]:
    for name, path in step.capture.items():
        value = resolve_path(result, path)
        if value is not MISSING:
            yield name, value


def _check_agreement(
    report: Report, scenario: Scenario, step: Step, active: Sequence[Binding]
) -> None:
    relevant = [
        outcome
        for outcome in report.outcomes
        if outcome.scenario == scenario.id and outcome.step == step.id
    ]
    by_binding = {outcome.binding: outcome for outcome in relevant}
    for path in step.agree:
        values: Dict[str, Any] = {}
        for binding in active:
            outcome = by_binding.get(binding.id)
            if outcome is None or outcome.result is None or not outcome.passed:
                continue
            value = resolve_path(outcome.result, path)
            if value is not MISSING:
                values[binding.id] = value
        if len(values) < 2:
            continue
        distinct = []
        for value in values.values():
            if not any(value == seen for seen in distinct):
                distinct.append(value)
        if len(distinct) <= 1:
            continue
        detail = ", ".join(
            f"{binding}={describe_value(value)}" for binding, value in values.items()
        )
        for binding, outcome in by_binding.items():
            if outcome.passed and binding in values:
                outcome.failures.append(
                    f"{path}: bindings disagree ({detail})"
                )


def bindings_summary(report: Report) -> str:
    """One line for the run header, naming the bindings that ran."""
    if report.missing_bindings:
        return (
            f"bindings: {', '.join(report.bindings)} "
            f"(missing: {', '.join(report.missing_bindings)})"
        )
    return "bindings: " + ", ".join(report.bindings)


def format_report(report: Report) -> str:
    """Human-readable report: one line per scenario and binding, then failures."""
    lines = [
        f"conformance suite: {len(report.scenarios)} scenarios, "
        f"{len(report.outcomes)} step results",
        bindings_summary(report),
    ]
    per_scenario: Dict[str, Dict[str, List[StepOutcome]]] = {}
    for outcome in report.outcomes:
        per_scenario.setdefault(outcome.scenario, {}).setdefault(
            outcome.binding, []
        ).append(outcome)
    for scenario_id in report.scenarios:
        statuses = []
        for binding in report.bindings:
            outcomes = per_scenario.get(scenario_id, {}).get(binding)
            if not outcomes:
                statuses.append(f"{binding}=not-run")
            elif all(outcome.passed for outcome in outcomes):
                statuses.append(f"{binding}=pass")
            else:
                statuses.append(f"{binding}=failed")
        verdict = "PASS" if all(part.endswith("=pass") for part in statuses) else "FAIL"
        lines.append(f"  {verdict} {scenario_id}: {', '.join(statuses)}")
    failures = report.failures
    if failures:
        lines.append("")
        lines.append("failures:")
        for outcome in failures:
            reason = outcome.skipped or (
                outcome.failures[0] if outcome.failures else "unknown failure"
            )
            lines.append(
                f"  {outcome.scenario} / {outcome.step} [{outcome.binding}]: {reason}"
            )
            for extra in outcome.failures[1:]:
                lines.append(f"    also: {extra}")
    lines.append("")
    if report.ok:
        lines.append("result: PASS (all bindings, all scenarios)")
    elif not report.coverage_complete:
        lines.append(
            "result: NOT A CLEAN PASS (missing bindings: "
            + ", ".join(report.missing_bindings)
            + ")"
        )
    else:
        lines.append(
            f"result: FAIL ({len(failures)} of {len(report.outcomes)} step results)"
        )
    return "\n".join(lines)
