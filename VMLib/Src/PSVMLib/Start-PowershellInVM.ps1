<# 
.SYNOPSIS
	Executes a powershell script in VM.
.DESCRIPTION
	Executes a powershell script in VM.
.PARAMETER Code
	Script block to execute.
.PARAMETER Arguments
	Array of objects that can be accessed in script as $Args[0]..[1] etc.
.PARAMETER Credential
	Credentials for the guest operating system to run the password as.
.EXAMPLE
	Remove install share from vm.
	$vm | Start-PowershellInVM -Arguments @("foo", 1) -Credential (Get-Credential) {  Write-Host 'Do script stuff here on guest' }
.OUTPUTS
	Output from powershell script.
.INPUTS
	IVirtualMachine
#> 
[CmdletBinding]
function Start-PowershellInVM
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$VM, 
		  [Paramter(Mandatory=$true)][ScriptBlock]$Code,
		  [Paramter(Mandatory=$true)]$Arguments = @(),
		  [Paramter(Mandatory=$true)][PSCredential]$Credential)

	Process 
	{

	}
}