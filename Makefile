ANDROID_STUDIO_JAVA_HOME ?= /Applications/Android Studio.app/Contents/jbr/Contents/Home
GRADLE_WRAPPER := ./lasco-android/gradlew
DESKTOP_IMPORTER_PROJECT := lasco-desktop-importer
CARGO ?= cargo

.PHONY: desktop-importer
desktop-importer:
	@test -x "$(ANDROID_STUDIO_JAVA_HOME)/bin/java" || \
		(echo "Android Studio JDK not found at $(ANDROID_STUDIO_JAVA_HOME)" >&2; exit 1)
	$(CARGO) build -p lasco-ffi --release
	JAVA_HOME="$(ANDROID_STUDIO_JAVA_HOME)" \
		$(GRADLE_WRAPPER) -p $(DESKTOP_IMPORTER_PROJECT) run
