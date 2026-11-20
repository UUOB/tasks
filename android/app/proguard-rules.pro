# The core's Kotlin bindings call into Rust through JNA, which finds classes by name.
-keep class com.sun.jna.** { *; }
-keep class * extends com.sun.jna.** { *; }
-keep class app.uuob.client.** { *; }
-keep class app.uuob.photos.core.** { *; }
-dontwarn java.awt.**
