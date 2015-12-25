<#	
	===========================================================================
	 Created on:   	25/12/2015 9:48 PM
	 Created by:   	Wil Taylor (wilfridtaylor@gmail.com) 
	 Organization: 	 
	 Filename:     	NetMetered.psm1
	-------------------------------------------------------------------------
	 Module Name: NetMetered
	===========================================================================
#>

<# 
	.SYNOPSIS
	Returns if current connection is a metered connection or not.

	.DESCRIPTION
	This cmdlet checks if connection is metered or not.

	This is based on the example in https://msdn.microsoft.com/en-us/library/windows/apps/xaml/jj835821.aspx

	.EXAMPLE
	Check if connected to a metered connection

	If(Test-NetMetered) { Write-Host "Metered" }
#>
function Test-NetMetered
{
	[void][Windows.Networking.Connectivity.NetworkInformation, Windows, ContentType = WindowsRuntime]
	$networkprofile = [Windows.Networking.Connectivity.NetworkInformation]::GetInternetConnectionProfile()
	
	if ($networkprofile -eq $null)
	{
		Write-Warning "Can't find any internet connections!"
		return $false
	}
	
	$cost = $networkprofile.GetConnectionCost()
	
	
	if ($cost -eq $null)
	{
		Write-Warning "Can't find any internet connections with a cost!"
		return $false
	}
	
	if ($cost.Roaming -or $cost.OverDataLimit)
	{
		return $true
	}
	
	if ($cost.NetworkCostType -eq [Windows.Networking.Connectivity.NetworkCostType]::Fixed -or
	$cost.NetworkCostType -eq [Windows.Networking.Connectivity.NetworkCostType]::Variable)
	{
		return $true
	}
	
	if ($cost.NetworkCostType -eq [Windows.Networking.Connectivity.NetworkCostType]::Unrestricted)
	{
		return $false
	}
	
	throw "Network cost type is unknown!"
	
}
Export-ModuleMember Test-NetMetered



