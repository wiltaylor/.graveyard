<# 
.SYNOPSIS
	Starts a process in virtual machine.
.DESCRIPTION
	Starts a process in the virtual machine.
.PARAMETER Path
	Path to executable in guest operating system.
.PARAMETER Arguments
	Arguments to pass to executable in guest operating system.
.PARAMETER Wait
	Wait for executable to exit.
.PARAMETER Interactive
	Run executable as an interactive process in the guest operating system.
.PARAMETER Credential
	Credentials for the guest operating system to run the password as.
.EXAMPLE
	Remove install share from vm.
	$vm | Start-ProcessInVM -Path "c:\tools\myutil.exe" -Arguments "-dosomething" -Wait -Credential (Get-Credential)
.INPUTS
	IVirtualMachine
#> 
[CmdletBinding]
function Start-ProcessInVM
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$VM, 
		  [Paramter(Mandatory=$true)][string]$Path,
		  [Paramter(Mandatory=$true)][string]$Arguments,
		  [Paramter(Mandatory=$true)][PSCredential]$Credential,
		  [switch]$Wait, [switch]$Interactive)

	Process 
	{

	}
}