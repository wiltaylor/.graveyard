<# 
.SYNOPSIS
	Executes a powershell script in VM.
.DESCRIPTION
	Executes a powershell script in VM.
.PARAMETER Code
	Script block to execute.
.PARAMETER Username
	Guest OS username to run the powershell script as.
.PARAMETER Password
	Guest OS password to run the powershell script as.
.EXAMPLE
	Remove install share from vm.
	$vm | Start-PowershellInVM -Arguments @("foo", 1) -Username MyUsername -Password SecretPassword {  Write-Host 'Do script stuff here on guest' }
.OUTPUTS
	Output from powershell script.
.INPUTS
	IVirtualMachine
#> 
[CmdletBinding]
function Invoke-PowershellInVM
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$VM, 
		  [Parameter(Mandatory=$true)][ScriptBlock]$Code,
		  [Parameter(Mandatory=$true)][string]$Username,
		  [Parameter(Mandatory=$true)][string]$Password)

	Process 
	{
		$VM.Username = $Username
		$VM.Password= $Password
		$VM.ExecutePowershell($Code.ToString())
	}
}