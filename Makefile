ANDROID_STUDIO_JAVA_HOME ?= /Applications/Android Studio.app/Contents/jbr/Contents/Home
PACKAGING_JAVA_HOME ?= /Library/Java/JavaVirtualMachines/jdk-22.jdk/Contents/Home
GRADLE_WRAPPER := ./lasco-android/gradlew
DESKTOP_IMPORTER_PROJECT := lasco-desktop-importer
DESKTOP_IMPORTER_APP := $(DESKTOP_IMPORTER_PROJECT)/build/compose/binaries/main/app/lasco-desktop-importer.app
CARGO ?= cargo

.PHONY: desktop-importer
desktop-importer:
	@test -x "$(PACKAGING_JAVA_HOME)/bin/jpackage" || \
		(echo "A full JDK with jpackage is required at $(PACKAGING_JAVA_HOME). Set PACKAGING_JAVA_HOME to one that provides bin/jpackage." >&2; exit 1)
	$(CARGO) build -p lasco-ffi --release
	JAVA_HOME="$(PACKAGING_JAVA_HOME)" \
		$(GRADLE_WRAPPER) -p $(DESKTOP_IMPORTER_PROJECT) createDistributable
	@open -R "$(DESKTOP_IMPORTER_APP)"
