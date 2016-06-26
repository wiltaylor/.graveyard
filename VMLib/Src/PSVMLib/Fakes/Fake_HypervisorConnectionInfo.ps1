function New-FakeHypervisorConnectionInfo 
{
	param([string]$Name = "")
	$fake = New-Object psobject
	$fake | Add-Member -MemberType NoteProperty -Name Name -Value $Name
	return $fake
}