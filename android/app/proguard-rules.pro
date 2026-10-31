# UniFFI's bindings reach the Rust library through JNA, by name.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class uniffi.fob.** { *; }
-dontwarn java.awt.**
