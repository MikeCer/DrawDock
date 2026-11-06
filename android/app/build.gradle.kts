plugins {
    id("com.android.application")
}

val releaseStore = providers.environmentVariable("DRAWDOCK_ANDROID_KEYSTORE")

android {
    namespace = "com.drawdock"
    compileSdk = 36

    defaultConfig {
        applicationId = "com.drawdock"
        minSdk = 30
        targetSdk = 36
        versionCode = providers.environmentVariable("DRAWDOCK_VERSION_CODE").orNull?.toInt() ?: 1
        versionName = providers.environmentVariable("DRAWDOCK_VERSION").orNull ?: "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    signingConfigs {
        if (releaseStore.isPresent) {
            create("release") {
                storeFile = file(releaseStore.get())
                storePassword = providers.environmentVariable("DRAWDOCK_ANDROID_STORE_PASSWORD").get()
                keyAlias = providers.environmentVariable("DRAWDOCK_ANDROID_KEY_ALIAS").get()
                keyPassword = providers.environmentVariable("DRAWDOCK_ANDROID_KEY_PASSWORD").get()
            }
        }
    }
    buildTypes {
        release {
            if (releaseStore.isPresent) signingConfig = signingConfigs.getByName("release")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    testOptions {
        unitTests.isReturnDefaultValues = true
    }
}

dependencies {
    implementation("androidx.activity:activity:1.10.1")
    implementation("androidx.core:core-splashscreen:1.2.0")
    implementation("com.journeyapps:zxing-android-embedded:4.3.0")
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.json:json:20240303")
}
