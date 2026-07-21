import org.gradle.api.tasks.Exec

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

android {
    namespace = "org.mininet.app"
    compileSdk = 36

    defaultConfig {
        applicationId = "org.mininet.app"
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "0.0.1-dev"
    }

    // A fixed, committed debug keystore (D-0349, issue #205) rather than
    // AGP's implicit default, which auto-generates a brand-new random
    // debug key on any machine that has never built this project before
    // -- exactly what made two independent GitHub Actions runners produce
    // non-reproducible signatures. Standard AOSP debug-key alias/password
    // ("androiddebugkey"/"android"); this is not a secret -- every Android
    // developer's default ~/.android/debug.keystore uses the identical
    // well-known convention, and this key never signs anything but debug
    // builds.
    signingConfigs {
        getByName("debug") {
            storeFile = file("debug.keystore")
            storePassword = "android"
            keyAlias = "androiddebugkey"
            keyPassword = "android"
        }
    }

    buildTypes {
        getByName("debug") {
            signingConfig = signingConfigs.getByName("debug")
        }
    }

    buildFeatures {
        compose = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    packaging {
        resources.excludes += "/META-INF/{AL2.0,LGPL2.1}"
    }
}

val repositoryRoot = rootDir.resolve("../..").canonicalFile
val generatedUniFfi = layout.buildDirectory.dir("generated/source/uniffi")

val generateUniFfiKotlin by tasks.registering(Exec::class) {
    group = "mininet"
    description = "Generate Kotlin bindings from the reviewed mini-ffi UDL."
    workingDir(repositoryRoot)

    val udl = repositoryRoot.resolve("crates/mini-ffi/src/mini_ffi.udl")
    val config = repositoryRoot.resolve("crates/mini-ffi/uniffi-bindgen.toml")
    inputs.files(udl, config)
    outputs.dir(generatedUniFfi)

    commandLine(
        "cargo",
        "run",
        "-p",
        "mini-ffi",
        "--features",
        "bindgen",
        "--bin",
        "uniffi-bindgen",
        "--",
        "generate",
        udl.absolutePath,
        "--language",
        "kotlin",
        "--out-dir",
        generatedUniFfi.get().asFile.absolutePath,
        "--config",
        config.absolutePath,
        "--no-format",
    )
}

android.sourceSets.getByName("main").kotlin.srcDir(generatedUniFfi.get().asFile)
tasks.named("preBuild").configure { dependsOn(generateUniFfiKotlin) }

dependencies {
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.ui.tooling.preview)
    implementation(libs.androidx.compose.material3)
    implementation("net.java.dev.jna:jna:5.17.0@aar")
    debugImplementation(libs.androidx.compose.ui.tooling)
}
