function New-FakeVM
{
	param($State, $Snapshots, $Start = {}, $Stop = {}, $Restart = {}, $CreateSnapshot = {}, $RestoreSnapshot = {}, $RemoveSnapshot = {}, 
	$AddSharedFolder = {}, $Username, $Password, $Processes, $HypervisorName, $RemoteAccessProtocol, $RemoteAccessPort,
	$RemoteAccessPassword, $RemoveSharedFolder = {}, $ExecuteCommand = {}, $ExecutePowershell = {}, $KillProcess = {}, $CopyToVM = {}, 
	$CopyFromVM = {}, $FileExists = {}, $DirectoryExists = {}, $DeleteFile = {}, $DeleteDirectory = {}, $WaitTillReady = {}, $WaitTillOff = {},
	$ReadEnvironment = {}, $ReadGuestVariable = {}, $ReadVMSetting = {}, $WriteEnvironment = {}, $WriteGuestVariable = {}, $WriteVMSetting = {}, 
	$AddNetworkCard = {}, $GetNetworkCards = {}, $RemoveNetworkCard = {}, $AddDisk = {}, $GetDisks = {}, $RemoveDisk = {}, $OpenLocalGUI = {},
	$CreateRemoteConnection = {}, $RemoveRemoteConnection = {})

	$fake = New-Object psobject
	$fake | Add-Member -MemberType NoteProperty -Name State -Value $State
	$fake | Add-Member -MemberType NoteProperty -Name Snapshots -Value $Snapshots
	$fake | Add-Member -MemberType ScriptMethod -Name Start -Value $Start
	$fake | Add-Member -MemberType ScriptMethod -Name Stop -Value $Stop
	$fake | Add-Member -MemberType ScriptMethod -Name Restart -Value $Restart
	$fake | Add-Member -MemberType ScriptMethod -Name CreateSnapshot -Value $CreateSnapshot
	$fake | Add-Member -MemberType ScriptMethod -Name RestoreSnapshot -Value $RestoreSnapshot
	$fake | Add-Member -MemberType ScriptMethod -Name RemoveSnapshot -Value $RemoveSnapshot
	$fake | Add-Member -MemberType ScriptMethod -Name AddSharedFolder -Value $AddSharedFolder
	$fake | Add-Member -MemberType NoteProperty -Name Username -Value $Username
	$fake | Add-Member -MemberType NoteProperty -Name Password -Value $Password
	$fake | Add-Member -MemberType NoteProperty -Name Processes -Value $Processes
	$fake | Add-Member -MemberType NoteProperty -Name HypervisorName -Value $HypervisorName
	$fake | Add-Member -MemberType NoteProperty -Name RemoteAccessProtocol -Value $RemoteAccessProtocol
	$fake | Add-Member -MemberType NoteProperty -Name RemoteAccessPort -Value $RemoteAccessPort
	$fake | Add-Member -MemberType NoteProperty -Name RemoteAccessPassword -Value $RemoteAccessPassword
	$fake | Add-Member -MemberType ScriptMethod -Name RemoveSharedFolder -Value $RemoveSharedFolder
	$fake | Add-Member -MemberType ScriptMethod -Name ExecuteCommand -Value $ExecuteCommand
	$fake | Add-Member -MemberType ScriptMethod -Name ExecutePowershell -Value $ExecutePowershell
	$fake | Add-Member -MemberType ScriptMethod -Name KillProcess -Value $KillProcess
	$fake | Add-Member -MemberType ScriptMethod -Name CopyToVM -Value $CopyToVM
	$fake | Add-Member -MemberType ScriptMethod -Name CopyFromVM -Value $CopyFromVM
	$fake | Add-Member -MemberType ScriptMethod -Name FileExists -Value $FileExists
	$fake | Add-Member -MemberType ScriptMethod -Name DirectoryExists -Value $DirectoryExists
	$fake | Add-Member -MemberType ScriptMethod -Name DeleteFile -Value $DeleteFile
	$fake | Add-Member -MemberType ScriptMethod -Name DeleteDirectory -Value $DeleteDirectory
	$fake | Add-Member -MemberType ScriptMethod -Name WaitTillReady -Value $WaitTillReady
	$fake | Add-Member -MemberType ScriptMethod -Name WaitTillOff -Value $WaitTillOff
	$fake | Add-Member -MemberType ScriptMethod -Name ReadEnvironment -Value $ReadEnvironment
	$fake | Add-Member -MemberType ScriptMethod -Name ReadGuestVariable -Value $ReadGuestVariable
	$fake | Add-Member -MemberType ScriptMethod -Name ReadVMSetting -Value $ReadVMSetting
	$fake | Add-Member -MemberType ScriptMethod -Name WriteEnvironment -Value $WriteEnvironment
	$fake | Add-Member -MemberType ScriptMethod -Name WriteGuestVariable -Value $WriteGuestVariable
	$fake | Add-Member -MemberType ScriptMethod -Name WriteVMSetting -Value $WriteVMSetting
	$fake | Add-Member -MemberType ScriptMethod -Name AddNetworkCard -Value $AddNetworkCard
	$fake | Add-Member -MemberType ScriptMethod -Name GetNetworkCards -Value $GetNetworkCards
	$fake | Add-Member -MemberType ScriptMethod -Name RemoveNetworkCard -Value $RemoveNetworkCard
	$fake | Add-Member -MemberType ScriptMethod -Name AddDisk -Value $AddDisk
	$fake | Add-Member -MemberType ScriptMethod -Name GetDisks -Value $GetDisks
	$fake | Add-Member -MemberType ScriptMethod -Name RemoveDisk -Value $RemoveDisk
	$fake | Add-Member -MemberType ScriptMethod -Name OpenLocalGUI -Value $OpenLocalGUI
	$fake | Add-Member -MemberType ScriptMethod -Name CreateRemoteConnection -Value $CreateRemoteConnection
	$fake | Add-Member -MemberType ScriptMethod -Name RemoveRemoteConnection -Value $RemoveRemoteConnection
	return $fake
}