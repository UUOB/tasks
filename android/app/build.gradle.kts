// Tasks for Android: Jetpack Compose screens over the Rust core (core/) and
// UUOB's SDK (uuob/crates/uuob-client). android/build.sh first builds the core
// (src/main/jniLibs) and its Kotlin bindings (src/main/java/app/uuob/{client,tasks/core}),
// then runs Gradle. The connect screen, theme and updater are the apps' kit (sdk/kit/android, UUOB-sdk).

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

// Tasks' version lives in one place, Cargo.toml.
val appVersion = Regex("^version = \"(.+)\"", RegexOption.MULTILINE)
    .find(rootProject.file("../Cargo.toml").readText())!!.groupValues[1]
val (major, minor, patch) = appVersion.split(".").map { it.toInt() }

android {
    namespace = "app.uuob.tasks"
    compileSdk = 37
    defaultConfig {
        applicationId = "app.uuob.tasks"
        minSdk = 26
        targetSdk = 36
        // As the Tauri app numbered them (0.3.2 was 3002), so this installs over it.
        versionCode = major * 1_000_000 + minor * 1_000 + patch
        versionName = appVersion
    }
    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }
    buildFeatures { compose = true; buildConfig = true }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    // UUOB's kit for apps: the connect screen, theme and updater.
    sourceSets["main"].kotlin.srcDir("../../sdk/kit/android/src")
    packaging { jniLibs { useLegacyPackaging = false } }
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2026.09.00"))
    implementation("androidx.compose.material3:material3:1.5.0-rc01")
    implementation("androidx.compose.material:material-icons-extended:1.7.8")
    implementation("androidx.activity:activity-compose:1.13.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.11.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.11.0")
    // The Kotlin bindings (UniFFI) reach the core through JNA.
    implementation("net.java.dev.jna:jna:5.19.1@aar")
}
