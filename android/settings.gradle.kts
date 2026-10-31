pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "fob"

// app: the Compose app, a thin layer over the Rust wallet (crates/fob-core
// through crates/fob-ffi), built into it by the app's cargo tasks.
include(":app")
