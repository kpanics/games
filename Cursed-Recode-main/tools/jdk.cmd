@rem Sets JDK to a Java 25 installation. Called from build.cmd / run.cmd (no setlocal here on purpose).
@set "JDK=C:\Program Files\Eclipse Adoptium\jdk-25.0.4.7-hotspot"
@if exist "%JDK%\bin\javac.exe" goto :ok
@set "JDK=%JAVA_HOME%"
@if exist "%JDK%\bin\javac.exe" goto :ok
@echo [jdk] JDK 25 not found. Set JAVA_HOME to a JDK 25.
@exit /b 1
:ok
@exit /b 0
