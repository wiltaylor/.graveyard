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
function Invoke-ProcessInVM
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$VM, 
		  [Parameter(Mandatory=$true)][string]$Path,
		  [Parameter(Mandatory=$true)][string]$Arguments,
		  [Parameter(Mandatory=$true)][string]$Username,
		  [Parameter(Mandatory=$true)][string]$Password,
		  [switch]$Wait, [switch]$Interactive)

	Process 
	{
		$VM.Username = $Username
		$VM.Password= $Password
		$VM.ExecuteCommand($path, $args, $Wait, $Interactive)
	}
}