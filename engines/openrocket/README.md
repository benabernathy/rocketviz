# OpenRocket engine for rocketviz

This engine runs [OpenRocket](https://openrocket.info) 24.12 simulations for rocketviz.

- **Protocol:** speaks the rocketviz engine protocol ([`docs/engine-protocol.md`](../../docs/engine-protocol.md)): JSON-RPC 2.0 over stdin/stdout.
- **License:** GPL-3.0-or-later, because it links against OpenRocket (see `LICENSE`). The rest of rocketviz is Apache-2.0.

## Build and test
Needs Java 21. Gradle downloads one if it's missing.
```
./gradlew test             # golden parity, protocol, batch and process tests
./gradlew assembleEngine   # build/engine/: engine.json, engine.jar, OpenRocket JAR, LICENSE
./gradlew bundle           # build/engine/ plus build/jre/, the trimmed Java runtime the desktop app ships
./gradlew bundle test      # also checks the bundle against a full JDK (BundleParityTest)
```
The bundled OpenRocket JAR has its native libraries removed. They belong to the desktop UI (JOGL, JNA, FlatLaf), which the headless engine never loads, and Apple's notarization rejects unsigned native code even inside a JAR. `BundleParityTest` checks that OpenRocket's example designs simulate identically with and without them.
The first build downloads `OpenRocket-24.12.jar` into `lib/` and verifies its SHA-256.

## Try it by hand
```
cd build/engine
java -Djava.awt.headless=true -cp "engine.jar:OpenRocket-24.12.jar" net.rocketviz.engine.openrocket.Main
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocol_version":1}}
{"jsonrpc":"2.0","id":2,"method":"load","params":{"path":"/path/to/Fizz.ork"}}
{"jsonrpc":"2.0","id":3,"method":"run","params":{"design_id":"d1","config_id":"0","output":"landing"}}
```

## Notes on OpenRocket behavior
- **Turbulence seeding:** OpenRocket seeds its turbulence model randomly when a design loads. Its "random seed" option never reaches that model, so results with the same seed would differ between runs of the app.
  - The bridge replaces the wind model with one built from the requested seed, or from seed 0 when none is given.
  - Results are therefore reproducible in any process. `GoldenParityTest` checks them against reference values.
- **Rod direction:** with "launch into wind" on, OpenRocket ignores the rod direction. An explicit `rod_direction` override turns launch-into-wind off, so the override means what it says.
- **Parallelism:** OpenRocket stops getting faster beyond about 3–4 threads in one JVM, because of global state inside its simulation code.
  - So each process uses at most 3 batch threads, and declares `multi_instance` so the host can run several processes, which scale nearly linearly.
  - Measured with 150 Monte Carlo runs on an Apple M-series machine:

    | Setup | Time | RAM |
    |---|---|---|
    | 1 thread (sequential) | 3.3 s | — |
    | 2 processes × 3 threads | 1.5 s | 0.93 GB |
    | 6 processes × 1 thread | 0.9 s | 2.1 GB |

- **JVM settings** (in `engine.json`): `-Xmx320m -XX:+UseSerialGC -XX:CICompilerCount=2` keeps each process near 470 MB. With the default JVM settings it's about 830 MB.
