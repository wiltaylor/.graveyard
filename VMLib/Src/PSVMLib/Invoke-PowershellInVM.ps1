<# 
.SYNOPSIS
	Executes a powershell script in VM.
.DESCRIPTION
	Executes a powershell script in VM.
.PARAMETER Code
	Script block to execute.
.PARAMETER Arguments
	Hashtable of arguments to pass to script. These will be serialised and copied into the virtual environment.
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
		  [Parameter(Mandatory=$false)][HashTable]$Arguments,
		  [Parameter(Mandatory=$true)][string]$Username,
		  [Parameter(Mandatory=$true)][string]$Password)

	Process 
	{
		$VM.Username = $Username
		$VM.Password= $Password
		$result = $VM.ExecutePowershell($Code.ToString(), $Arguments)

		foreach($i in $result.Result) { Write-Output $i }

		foreach($i in $result.Error) { Write-Error $i }
	}
}