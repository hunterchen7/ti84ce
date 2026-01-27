# Add project specific ProGuard rules here.
# By default, the flags in this file are appended to flags specified
# in /sdk/tools/proguard/proguard-android.txt

# Keep native methods
-keepclasseswithmembernames class * {
    native <methods>;
}

# Keep the EmulatorBridge class
-keep class com.calc.emulator.EmulatorBridge { *; }
