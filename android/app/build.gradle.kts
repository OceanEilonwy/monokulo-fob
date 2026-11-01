plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

// The wallet is the Rust crate fob-core, through fob-ffi's UniFFI bindings.
// Both the native libraries and the Kotlin bindings are built here from the
// crates, so they can't drift: building needs Rust, cargo-ndk and the NDK.
val repo: File = rootDir.parentFile
val hostLibrary = File(repo, "target/release/" + System.mapLibraryName("fob_ffi"))
// Phones, and the emulator.
val abis = listOf("arm64-v8a", "x86_64")

/** A cargo command that writes one directory, so AGP can take it as generated sources. */
abstract class CargoTask : Exec() {
    @get:OutputDirectory
    abstract val outDir: DirectoryProperty
}

android {
    namespace = "com.monokulo.fob"
    compileSdk = 37
    compileSdkMinor = 2
    ndkVersion = "27.0.12077973"

    defaultConfig {
        applicationId = "com.monokulo.fob"
        minSdk = 26
        targetSdk = 37
        // A release build is given these by CI (.github/workflows/release.yml).
        versionCode = (project.findProperty("fobVersionCode") as String?)?.toInt() ?: 1
        versionName = (project.findProperty("fobVersion") as String?) ?: "0.1.1"
    }

    // Only a build given the keystore (CI's release secrets) is signed; any
    // other release build stays unsigned.
    signingConfigs {
        System.getenv("FOB_KEYSTORE")?.let { keystore ->
            create("release") {
                storeFile = file(keystore)
                storePassword = System.getenv("FOB_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("FOB_KEY_ALIAS")
                keyPassword = System.getenv("FOB_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        debug {
            applicationIdSuffix = ".debug"
        }
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.findByName("release")
        }
    }

    buildFeatures {
        compose = true
    }
}

val cargoHost = tasks.register<Exec>("cargoHost") {
    description = "Builds fob-ffi for this machine, to generate the bindings from."
    workingDir = repo
    commandLine("cargo", "build", "--release", "--locked", "-p", "fob-ffi")
    inputs.dir(File(repo, "crates"))
    inputs.dir(File(repo, "third_party"))
    inputs.files(File(repo, "Cargo.lock"), File(repo, "Cargo.toml"))
    outputs.file(hostLibrary)
}

val uniffiBindings = tasks.register<CargoTask>("uniffiBindings") {
    description = "Generates fob-ffi's Kotlin bindings."
    dependsOn(cargoHost)
    workingDir = repo
    outDir.set(layout.buildDirectory.dir("rust/kotlin"))
    inputs.file(hostLibrary)
    argumentProviders.add(CommandLineArgumentProvider { listOf(outDir.get().asFile.absolutePath) })
    commandLine(
        "cargo", "run", "--release", "--locked", "-q", "-p", "fob-ffi", "--features", "bindgen",
        "--bin", "uniffi-bindgen", "--", "generate", "--library", hostLibrary.absolutePath,
        "--language", "kotlin", "--no-format", "--out-dir",
    )
}

val cargoNdk = tasks.register<CargoTask>("cargoNdk") {
    description = "Builds fob-ffi for each Android ABI."
    workingDir = repo
    outDir.set(layout.buildDirectory.dir("rust/jniLibs"))
    inputs.dir(File(repo, "crates"))
    inputs.dir(File(repo, "third_party"))
    inputs.files(File(repo, "Cargo.lock"), File(repo, "Cargo.toml"))
    // This module's NDK, whatever the environment says.
    val ndk = androidComponents.sdkComponents.ndkDirectory.map { it.asFile.absolutePath }
    doFirst {
        environment("ANDROID_NDK_HOME", ndk.get())
        environment("NDK_HOME", ndk.get())
    }
    commandLine(listOf("cargo", "ndk") + abis.flatMap { listOf("-t", it) } + listOf("--platform", "26", "-o"))
    argumentProviders.add(
        CommandLineArgumentProvider {
            listOf(outDir.get().asFile.absolutePath, "build", "--profile", "android", "--locked", "-p", "fob-ffi")
        },
    )
}

androidComponents {
    onVariants { variant ->
        variant.sources.jniLibs?.addGeneratedSourceDirectory(cargoNdk, CargoTask::outDir)
        variant.sources.kotlin?.addGeneratedSourceDirectory(uniffiBindings, CargoTask::outDir)
    }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.biometric)
    implementation(libs.androidx.fragment)
    implementation(libs.lifecycle.viewmodel.compose)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.kotlinx.coroutines.android)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.material3)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.camerax.camera2)
    implementation(libs.camerax.lifecycle)
    implementation(libs.camerax.view)
    implementation(libs.zxing.core)
    // JNA as an AAR for the app: it carries JNA's own native library.
    implementation("${libs.jna.get()}@aar")
    debugImplementation(libs.compose.ui.tooling)
}
