@echo off
chcp 65001 >nul
title DeepSeek Balance Monitor — Setup

echo ==============================================
echo   DeepSeek Balance Monitor — Setup
echo ==============================================
echo.

:: Check Python
python --version >nul 2>&1
if %errorlevel% neq 0 (
    echo [ERROR] Python not found. Please install Python 3.9+ from https://python.org
    echo          Make sure to check "Add Python to PATH" during installation.
    pause
    exit /b 1
)

echo [OK] Python detected:
python --version
echo.

:: Install dependencies
echo [*] Installing dependencies...
pip install -r requirements.txt --quiet
if %errorlevel% neq 0 (
    echo [ERROR] Failed to install dependencies.
    pause
    exit /b 1
)
echo [OK] Dependencies installed.
echo.

:: Done
echo ==============================================
echo   Setup complete!
echo.
echo   To run the app:
echo     python main.py
echo.
echo   To run in background (no console window):
echo     pythonw main.py
echo.
echo   Tip: Place a shortcut in your Startup folder
echo        to auto-run on login.
echo ==============================================
pause
