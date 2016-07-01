<# 
.SYNOPSIS
	Stops a process running in VM.
.DESCRIPTION
	Stops target process running in VM.
.EXAMPLE
	Stops all processes in $processes on vm.
	$processes | Stop-ProcessInVM
.INPUTS
	IVMProcess
#> 
[CmdletBinding]
function Stop-ProcessInVM
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$Process)

	Process 
	{
		$vm = $Process.VM
		$procid = $Process.ProcessID

		$vm.KillProcess($procid)
	}
}