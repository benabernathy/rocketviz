import java.net.URI
import java.security.MessageDigest

plugins {
    java
}

group = "net.rocketviz"
version = "0.1.0"

java {
    toolchain { languageVersion = JavaLanguageVersion.of(21) }
}

// OpenRocket isn't on Maven Central; fetch the release JAR once and verify it.
val openRocketVersion = "24.12"
val openRocketSha256 = "4959b72f52f5f607941e9722abbb7b7f0c4a38ebbbf84204a329db9f31c4f897"
val openRocketJar = layout.projectDirectory.file("lib/OpenRocket-$openRocketVersion.jar")

val fetchOpenRocket = tasks.register("fetchOpenRocket") {
    description = "Downloads the OpenRocket release JAR into lib/ and verifies its SHA-256."
    outputs.file(openRocketJar)
    doLast {
        val target = openRocketJar.asFile
        fun sha256(f: File) = MessageDigest.getInstance("SHA-256").digest(f.readBytes()).joinToString("") { "%02x".format(it) }
        if (target.exists() && sha256(target) == openRocketSha256) return@doLast
        target.parentFile.mkdirs()
        val url = "https://github.com/openrocket/openrocket/releases/download/release-$openRocketVersion/OpenRocket-$openRocketVersion.jar"
        logger.lifecycle("Downloading $url")
        URI(url).toURL().openStream().use { input -> target.outputStream().use { input.copyTo(it) } }
        val actual = sha256(target)
        check(actual == openRocketSha256) { "OpenRocket JAR checksum mismatch: $actual" }
    }
}

repositories {
    mavenCentral()
}

dependencies {
    compileOnly(files(openRocketJar).builtBy(fetchOpenRocket))
    implementation("com.google.code.gson:gson:2.14.0")

    testImplementation(files(openRocketJar).builtBy(fetchOpenRocket))
    testImplementation(platform("org.junit:junit-bom:6.1.3"))
    testImplementation("org.junit.jupiter:junit-jupiter")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

tasks.withType<JavaCompile>().configureEach {
    options.release = 21
    options.compilerArgs.addAll(listOf("-Xlint:all", "-Xlint:-processing", "-Werror"))
}

// engine.jar = the bridge plus Gson. OpenRocket stays a separate JAR on the classpath (see engine.json).
tasks.jar {
    archiveFileName = "engine.jar"
    manifest { attributes("Main-Class" to "net.rocketviz.engine.openrocket.Main") }
    from(configurations.runtimeClasspath.map { cp -> cp.map { zipTree(it) } }) {
        exclude("META-INF/*.SF", "META-INF/*.DSA", "META-INF/*.RSA", "META-INF/versions/**", "module-info.class")
    }
    duplicatesStrategy = DuplicatesStrategy.EXCLUDE
}

// OpenRocket's JAR without its native libraries (JOGL, JNA, FlatLaf: desktop UI only). The engine is headless and never
// loads them, and Apple's notarization rejects unsigned native code even inside a JAR. Same file name, so engine.json
// doesn't change. Verified against OpenRocket's example designs: identical results with and without them.
val headlessOpenRocketJar = tasks.register<Zip>("headlessOpenRocketJar") {
    description = "OpenRocket's JAR minus native libraries, for bundling."
    dependsOn(fetchOpenRocket)
    from(zipTree(openRocketJar)) {
        exclude("**/*.dylib", "**/*.jnilib", "**/*.so", "**/*.dll")
    }
    archiveFileName = "OpenRocket-$openRocketVersion.jar"
    destinationDirectory = layout.buildDirectory.dir("headless")
    isZip64 = true
    isPreserveFileTimestamps = false
    isReproducibleFileOrder = true
}

// The installable engine folder the app bundles: build/engine/
tasks.register<Sync>("assembleEngine") {
    description = "Assembles the engine folder (engine.json, engine.jar, OpenRocket JAR, licenses)."
    from(tasks.jar)
    from(headlessOpenRocketJar)
    from("engine.json", "LICENSE", "README.md")
    into(layout.buildDirectory.dir("engine"))
}

// The Java runtime the desktop app bundles: build/jre/ (about 47 MB), for the platform it's built on.
// Modules from `jdeps --print-module-deps` on engine.jar + OpenRocket, plus java.scripting (simulation scripting
// extensions) and jdk.zipfs. Checked by running every OpenRocket example design on it: identical to a full JDK.
// No CDS archive: it adds 25 MB and saves ~0.02 s of the engine's ~1.4 s start (OpenRocket's own loading dominates).
val runtimeModules = listOf(
    "java.base", "java.compiler", "java.desktop", "java.naming", "java.prefs", "java.scripting", "java.sql",
    "java.xml.crypto", "jdk.management", "jdk.unsupported", "jdk.zipfs",
)
tasks.register<Exec>("jlinkRuntime") {
    description = "Builds the trimmed Java runtime (jlink) the desktop app bundles."
    val jdk = javaToolchains.launcherFor { languageVersion = JavaLanguageVersion.of(21) }.map { it.metadata.installationPath }
    val output = layout.buildDirectory.dir("jre")
    inputs.property("modules", runtimeModules)
    outputs.dir(output)
    doFirst { delete(output) }
    executable = jdk.get().file("bin/jlink${if (System.getProperty("os.name").startsWith("Windows")) ".exe" else ""}").asFile.path
    args(
        "--add-modules", runtimeModules.joinToString(","),
        "--strip-debug", "--no-man-pages", "--no-header-files", "--compress=zip-6",
        "--output", output.get().asFile.path,
    )
    // jlink writes some legal notices read-only, which breaks re-copying the runtime into the app on the next build.
    doLast { output.get().asFile.walk().forEach { it.setWritable(true, true) } }
}

// Everything the desktop app bundles: build/engine/ and build/jre/.
tasks.register("bundle") {
    description = "Builds the engine folder and the Java runtime for the desktop app."
    dependsOn("assembleEngine", "jlinkRuntime")
}

tasks.test {
    useJUnitPlatform()
    // BundleParityTest checks the bundle when it has been built (`./gradlew bundle test`).
    mustRunAfter("bundle")
    systemProperty("bundle.build", layout.buildDirectory.get().asFile.path)
    jvmArgs("-Djava.awt.headless=true", "-Xmx1g")
    providers.systemProperty("golden.strict").orNull?.let { systemProperty("golden.strict", it) }
    // Show stderr so CI logs include the golden-parity differences per platform.
    testLogging { events("passed", "failed", "skipped"); showStandardStreams = true; exceptionFormat = org.gradle.api.tasks.testing.logging.TestExceptionFormat.FULL }
}
