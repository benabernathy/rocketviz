plugins {
    // Lets Gradle download a Java 21 toolchain on machines that don't have one.
    id("org.gradle.toolchains.foojay-resolver-convention") version "1.0.0"
}

rootProject.name = "rocketviz-openrocket-engine"
