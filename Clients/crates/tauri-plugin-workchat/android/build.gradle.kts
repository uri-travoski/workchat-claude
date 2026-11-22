import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

// The Google (FCM) path is compiled only when the app module has a google-services.json.
val hasFcm = rootProject.file("app/google-services.json").exists()

android {
    namespace = "app.workchat.mobile"
    compileSdk = 36

    defaultConfig {
        minSdk = 24
        consumerProguardFiles("consumer-rules.pro")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }

    sourceSets {
        getByName("main") {
            java.srcDir(if (hasFcm) "src/fcm/kotlin" else "src/nofcm/kotlin")
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_1_8
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("org.unifiedpush.android:connector:3.0.10")
    if (hasFcm) implementation("com.google.firebase:firebase-messaging:24.1.0")
    implementation(project(":tauri-android"))
}
