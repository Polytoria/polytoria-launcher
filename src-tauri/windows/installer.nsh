!include "LogicLib.nsh"
!include "WordFunc.nsh"

!insertmacro VersionCompare

!define VCREDIST_MIN_VERSION "14.44.35211.0"
!define VCREDIST_REGKEY "Software\Microsoft\VisualStudio\14.0\VC\Runtimes\x64"
!define VCREDIST_FILE "${__FILEDIR__}\VC_redist.x64.exe"

!macro NSIS_HOOK_PREINSTALL
	SetRegView 64
	ReadRegStr $R0 HKLM "${VCREDIST_REGKEY}" "Version"
	SetRegView lastused

	StrCpy $R1 $R0 1
	${If} $R1 == "v"
		StrCpy $R0 $R0 "" 1
	${EndIf}

	StrCpy $R2 "2"

	${If} $R0 != ""
		${VersionCompare} "$R0" "${VCREDIST_MIN_VERSION}" $R2
	${EndIf}

	${If} $R0 == ""
	${OrIf} $R2 == "2"
		StrCpy $R3 "0"
		${If} $R0 == ""
			StrCpy $R3 "1"
		${EndIf}

		DetailPrint "Installing Microsoft Visual C++ Redistributable..."
		InitPluginsDir
		File /oname=$PLUGINSDIR\vc_redist.x64.exe "${VCREDIST_FILE}"
		ClearErrors
		ExecShellWait "runas" "$PLUGINSDIR\vc_redist.x64.exe" '/install /quiet /norestart /log "$TEMP\Polytoria-vcredist-x64.log"' SW_HIDE
		Delete "$PLUGINSDIR\vc_redist.x64.exe"

		${If} ${Errors}
			${If} $R3 == "1"
				MessageBox MB_ICONSTOP|MB_OK "Polytoria requires the Microsoft Visual C++ Redistributable to be installed.$\r$\n$\r$\nSetup tried but couldn't start the installer. Try running the installer as an administrator or install it manually from https://aka.ms/vs/17/release/vc_redist.x64.exe"
			${Else}
				MessageBox MB_ICONSTOP|MB_OK "Polytoria requires version ${VCREDIST_MIN_VERSION} or later of the Microsoft Visual C++ Redistributable.$\r$\n$\r$\nSetup tried but couldn't start the installer. Try running the installer as an administrator or install it manually from https://aka.ms/vs/17/release/vc_redist.x64.exe"
			${EndIf}
			Abort
		${EndIf}

		SetRegView 64
		ReadRegStr $R0 HKLM "${VCREDIST_REGKEY}" "Version"
		SetRegView lastused

		StrCpy $R1 $R0 1
		${If} $R1 == "v"
			StrCpy $R0 $R0 "" 1
		${EndIf}

		${If} $R0 == ""
			MessageBox MB_ICONSTOP|MB_OK "Failed to install the Microsoft Visual C++ Redistributable.$\r$\n$\r$\nCheck $TEMP\Polytoria-vcredist-x64.log for details, or install it manually from https://aka.ms/vs/17/release/vc_redist.x64.exe and try again."
			Abort
		${EndIf}

		${VersionCompare} "$R0" "${VCREDIST_MIN_VERSION}" $R2

		${If} $R2 == "2"
			MessageBox MB_ICONSTOP|MB_OK "The Microsoft Visual C++ Redistributable is still outdated after installation.$\r$\n$\r$\nCheck $TEMP\Polytoria-vcredist-x64.log for details, or install the latest from https://aka.ms/vs/17/release/vc_redist.x64.exe and try again."
			Abort
		${EndIf}
	${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
	RMDir /r "$LOCALAPPDATA\polytoria-updater"
	RMDir /r "$TEMP\Polytoria"
!macroend
