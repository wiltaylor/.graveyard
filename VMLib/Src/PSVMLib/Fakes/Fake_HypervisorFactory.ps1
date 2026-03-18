function New-FakeHypervisorFactory
{
	param([scriptblock]$GetHypervisorNames = {}, [scriptblock]$CreateHypervisorConnectionInfo = {}, $CreateHypervisor = {})
	$result = New-Object psobject
	$result | Add-Member -MemberType ScriptMethod -Name GetHypervisorNames -Value $GetHypervisorNames
	$result | Add-Member -MemberType ScriptMethod -Name CreateHypervisorConnectionInfo -Value $CreateHypervisorConnectionInfo
	$result | Add-Member -MemberType ScriptMethod -Name CreateHypervisor -Value $CreateHypervisor
	return $result
}




