# rocketviz engine protocol, version 1

A rocketviz **engine** is a separate program that simulates rocket flights for the rocketviz app.

- The app (the **host**) starts the engine as a child process and talks to it with **JSON-RPC 2.0** over stdin and stdout.
- Any language works. OpenRocket's engine is written in Java (`engines/openrocket/`).
- This document is the contract between the host and an engine. It is licensed Apache-2.0 like the rest of rocketviz, so engines under any license can implement it.

## 1. Packaging and discovery

An engine is a folder. Its `engine.json` tells the host how to start the engine *without* running it:

```json
{
  "protocol": "rocketviz-engine/1",
  "id": "openrocket",
  "name": "OpenRocket",
  "version": "0.1.0",
  "license": "GPL-3.0-or-later",
  "input_formats": [{ "extensions": ["ork"], "description": "OpenRocket design" }],
  "launch": {
    "runtime": "java",
    "args": ["-cp", "${engine_dir}/engine.jar${path_sep}${engine_dir}/OpenRocket-24.12.jar", "net.rocketviz.engine.openrocket.Main"]
  },
  "instances": { "recommended": 2, "memory_mb": 470 }
}
```

**`launch.runtime`** names the program to run:
- `"java"`: the host's bundled Java runtime.
- `"python"`: a Python runtime.
- `"exe"`: a native executable. In that case `args[0]` is its path, relative to the engine folder.

**Placeholders** the host substitutes in `args`:
- `${engine_dir}`: the engine folder's absolute path.
- `${path_sep}`: the platform's path separator, `:` or `;`.

**`instances`** is optional and only meaningful when `capabilities.multi_instance` is true:
- `recommended`: how many copies of the engine the host should run by default.
- `memory_mb`: roughly how much RAM each copy uses, so the host can scale the pool to the machine.

Everything else about the engine (capabilities, parameters) comes from the `initialize` call.

## 2. Transport

**Messages:**
- One JSON-RPC 2.0 message per line, as UTF-8 JSON with no embedded newlines, terminated by `\n`.
- The host writes to the engine's stdin; the engine writes only protocol messages to stdout.

**Logging:**
- Engines log to **stderr**. The host captures stderr for diagnostics and never parses it.
- Anything on stdout that isn't a protocol message is a protocol error. Engines must make sure third-party libraries can't print to stdout.

**Concurrency:**
- The engine handles requests one at a time, in order, except for `$/cancelRequest`. That one must be handled even while a request is running.
- Engines may parallelize work *within* a request, as in `run_batch`.

**Lifecycle:**
1. The host sends `initialize`, then any number of requests.
2. The host sends `shutdown`, waits for the response, then sends the `exit` notification.
3. The engine exits after `exit`, or when stdin closes.

## 3. Conventions

**Units:**
- SI throughout. Angles are in **degrees**.
- Time is seconds from the start of the simulation.

**Position:**
- `e`: metres east of the launch pad.
- `n`: metres north of the launch pad.
- `alt`: metres above the pad (AGL).

**Wind direction** is the direction the wind blows *from* (90 = from the east).

**Missing values** in arrays are `null`.

**Keys and enumerations:**
- Keys are `snake_case`.
- Event types are UPPER_SNAKE_CASE: `LAUNCH`, `LIFTOFF`, `LAUNCHROD`, `BURNOUT`, `APOGEE`, `RECOVERY_DEVICE_DEPLOYMENT`, `GROUND_HIT`, `SIMULATION_END`. Engines may add their own event types.

## 4. Methods

### `initialize`
**Params:**
```json
{ "protocol_version": 1, "host": { "name": "rocketviz", "version": "0.1.0" } }
```

**Result:**
```json
{
  "protocol_version": 1,
  "engine": { "id": "openrocket", "name": "OpenRocket", "version": "24.12", "license": "GPL-3.0-or-later" },
  "capabilities": { "batch": true, "parallel": true, "max_workers": 3, "cancel": true, "multi_instance": true },
  "parameters": [
    { "key": "wind_speed", "name": "Wind speed", "unit": "m/s", "kind": "standard", "min": 0 }
  ]
}
```

- **`capabilities`:**
  - `parallel` and `max_workers`: whether, and with how many threads, the engine runs a batch's cases concurrently within one process.
  - `multi_instance`: the host may run several copies of the engine at once and split a batch across them.
    - Each copy is independent, so the host loads the design in each one.
    - A case must give the same result in any copy (§4, `run_batch`).
    - Use it when an engine scales better across processes than across threads.
- **`parameters`** lists every override the engine accepts.
  - `kind: "standard"` keys have the meaning defined in §5.
  - `kind: "engine"` keys are specific to this engine. They carry their own `name`, `unit`, and optional `min`, `max` and `description`.
- The host builds its Monte Carlo and settings UI from this list.
- If the host's `protocol_version` isn't supported, the engine returns error `-32010`.

### `load`
Opens a design file.

**Params:**
```json
{ "path": "/abs/path/Fizz.ork" }
```

**Result:**
```json
{
  "design_id": "d1",
  "name": "Fizz",
  "configs": [
    {
      "id": "0",
      "name": "Simulation 1",
      "motor": "[A8-5]",
      "defaults": {
        "wind_speed": 2.0, "wind_direction": 90.0, "turbulence_intensity": 0.1,
        "rod_angle": 0.0, "rod_direction": 90.0, "rod_length": 1.0,
        "launch_latitude": 38.95776, "launch_longitude": -104.706208, "launch_altitude": 1950.0
      }
    }
  ]
}
```

- A **config** is one runnable setup: in OpenRocket, a simulation with its motor configuration.
- `defaults` are the config's own launch conditions, using the parameter keys.
- `design_id` stays valid until `unload` or engine exit.

### `unload`
**Params:** `{ "design_id": "d1" }`. **Result:** `null`.

### `run`
Simulates one case.

**Params:**
```json
{ "design_id": "d1", "config_id": "0", "overrides": { "wind_speed": 4, "seed": 7 }, "output": "full" }
```

- **`overrides`:** optional. Any omitted parameter uses the config's default.
- **`output`:** one of:

| Value | What the result contains |
|---|---|
| `"landing"` | `summary` and `landing` |
| `"summary"` | adds `events` and `conditions` |
| `"track"` | adds the `t`, `e`, `n`, `alt` arrays |
| `"full"` (default) | adds `series` |

**Result:**
```json
{
  "summary": { "max_alt": 152.3, "max_speed": 69.9, "apogee_time": 5.4, "flight_time": 40.3, "landing_speed": 4.9, "rod_exit_speed": 22.4 },
  "landing": { "e": -65.4, "n": 0.0 },
  "events": [{ "type": "APOGEE", "t": 5.42 }],
  "conditions": { "wind_speed": 2.0, "wind_direction": 90.0 },
  "t": [0.0, 0.01],
  "e": [0.0, 0.0],
  "n": [0.0, 0.0],
  "alt": [0.0, 0.01],
  "series": [
    { "key": "total_velocity", "name": "Total velocity", "symbol": "Vt", "unit": "m/s", "group": "Position and Motion", "data": [0.0, 0.4] }
  ]
}
```

- **`conditions`** are the values actually used, after overrides, keyed like `defaults`.
- **`series`** is every quantity the engine recorded, aligned with `t`. Standard keys the host relies on, where available:
  - `altitude`
  - `total_velocity`, `vertical_velocity`
  - `total_acceleration`, `vertical_acceleration`
  - `mach_number`
  - `thrust`
  - `drag_force`
  - `mass`
  - `angle_of_attack`

### `run_batch`
Simulates many cases, for Monte Carlo and wind sweeps.

**Params:**
```json
{ "design_id": "d1", "config_id": "0", "output": "landing",
  "cases": [{ "id": "mc-0", "overrides": { "wind_speed": 2.3, "wind_direction": 84.0, "seed": 1001 } }] }
```

**Progress:** as each case finishes, the engine sends a `progress` notification (no `id`):
```json
{ "jsonrpc": "2.0", "method": "progress",
  "params": { "request_id": 12, "case_id": "mc-0", "done": 1, "total": 150, "result": { "summary": {}, "landing": {} } } }
```

**Result**, when all cases have finished:
```json
{ "results": [{ "case_id": "mc-0", "result": { } }], "failed": [{ "case_id": "mc-7", "message": "Simulation diverged" }] }
```

- **Order:** results come back in case order. Notifications arrive in completion order.
- **Failures:** a failed case doesn't fail the batch.
- **Parallelism:** engines with `capabilities.parallel` run cases concurrently.
- **Determinism:** a case's result depends only on the design, the config and its overrides (including `seed`). It must not depend on which thread or process ran it, the order, or what ran before. When `seed` is omitted, the engine uses a fixed default seed, so reopening a design reproduces the same flight.

### `shutdown` / `exit`
- `shutdown` (request, result `null`): stop accepting work and release resources.
- `exit` (notification): end the process.

### `$/cancelRequest`
**Notification:** `{ "id": 12 }`.

- Cancels a running request. A batch stops starting new cases.
- The cancelled request then fails with `-32800`.
- It's harmless if the request already finished.

## 5. Standard parameters

| Key | Unit | Meaning |
|---|---|---|
| `wind_speed` | m/s | Mean wind speed |
| `wind_direction` | deg | Direction the wind blows from (0 = N, 90 = E) |
| `turbulence_intensity` | fraction | Wind turbulence intensity (0.1 = 10 %) |
| `rod_angle` | deg | Launch rod tilt from vertical |
| `rod_direction` | deg | Direction the rod tilts toward |
| `rod_length` | m | Launch rod length |
| `launch_latitude` | deg | Launch site latitude |
| `launch_longitude` | deg | Launch site longitude |
| `launch_altitude` | m | Launch site altitude above sea level |
| `seed` | — | Random seed for turbulence and other random effects. The same seed gives the same result, in any process. |
| `time_step` | s | Simulation time step |

## 6. Errors

Errors use standard JSON-RPC codes:
- `-32700`: parse error
- `-32600`: invalid request
- `-32601`: method not found
- `-32602`: invalid params
- `-32603`: internal error

Plus these:

| Code | Meaning |
|---|---|
| `-32010` | Unsupported protocol version |
| `-32011` | Design file couldn't be loaded (`data.path`) |
| `-32012` | Unknown `design_id` or `config_id` |
| `-32013` | Simulation failed (`data.message`) |
| `-32014` | Unknown or out-of-range override (`data.key`) |
| `-32800` | Request cancelled |
