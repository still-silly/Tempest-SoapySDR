plugins {
    id("com.android.application")
}

android {
    namespace = "io.github.stillsilly.tempestsdr"
    compileSdk = 34

    defaultConfig {
        applicationId = "io.github.stillsilly.tempestsdr"
        minSdk = 26
        targetSdk = 34
        versionCode = 1
        versionName = "0.1.0"

        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }
}
