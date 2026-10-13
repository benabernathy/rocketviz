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

// The installable engine folder the app bundles: build/engine/
tasks.register<Sync>("assembleEngine") {
    description = "Assembles the engine folder (engine.json, engine.jar, OpenRocket JAR, licenses)."
    dependsOn(fetchOpenRocket)
    from(tasks.jar)
    from(openRocketJar)
    from("engine.json", "LICENSE", "README.md")
    into(layout.buildDirectory.dir("engine"))
}

tasks.test {
    useJUnitPlatform()
    jvmArgs("-Djava.awt.headless=true", "-Xmx1g")
    providers.systemProperty("golden.strict").orNull?.let { systemProperty("golden.strict", it) }
    // Show stderr so CI logs include the golden-parity differences per platform.
    testLogging { events("passed", "failed", "skipped"); showStandardStreams = true; exceptionFormat = org.gradle.api.tasks.testing.logging.TestExceptionFormat.FULL }
}
