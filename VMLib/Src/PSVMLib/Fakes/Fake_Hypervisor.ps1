function New-FakeHypervisor
{
	param($name)

	$fake = New-Object psobject
	$fake | Add-Member -MemberType NoteProperty -Name Name -Value $name
	return $fake
}