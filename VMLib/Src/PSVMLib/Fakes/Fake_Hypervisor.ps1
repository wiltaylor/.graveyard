function New-FakeHypervisor
{
	param($name, $SetConnectionSettings = {}, $CreateNewVM = {}, $OpenVM = {}, $GetAllRunningVM = {})

	$fake = New-Object psobject
	$fake | Add-Member -MemberType NoteProperty -Name Name -Value $name
	$fake | Add-Member -MemberType ScriptMethod -Name SetConnectionSettings -Value $SetConnectionSettings
	$fake | Add-Member -MemberType ScriptMethod -Name CreateNewVM -Value $CreateNewVM
	$fake | Add-Member -MemberType ScriptMethod -Name OpenVM -Value $OpenVM
	$fake | Add-Member -MemberType ScriptMethod -Name GetAllRunningVM -Value $GetAllRunningVM
	return $fake
}