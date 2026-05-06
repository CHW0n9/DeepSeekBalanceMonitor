' DeepSeek Balance Monitor — Silent Launcher
' Double-click this file to run the app without a console window.
'
' Usage:
'   1. Make sure setup.bat has been run first
'   2. Double-click this file or place in Startup folder
'
' To auto-start with Windows:
'   Win+R → shell:startup → paste shortcut to this file

Set objShell = CreateObject("WScript.Shell")
objShell.Run "pythonw main.py", 0, False
