<#	
	===========================================================================
	 Created with: 	SAPIEN Technologies, Inc., PowerShell Studio 2015 v4.2.93
	 Created on:   	28/09/2015 10:09 PM
	 Created by:   	Wil Taylor
	 Organization: 	
	 Filename:     	PSPipe.psm1
	-------------------------------------------------------------------------
	 Module Name: PSPipe
	===========================================================================
#>

<#
	.SYNOPSIS
	Tests if a pipe is open.

	.DESCRIPTION
	Tests if a pipe is open.

	.INPUT
	Name of pipe to test if open or not.

	.PARAMETER Name
	Name of pipe to test if open or not.

	.OUTPUTS
	Returns $true if pipe is active or $false if its not.

	.EXAMPLE
	C:\> Test-PSPipe -Name "TestPipe"

	.EXAMPLE
	c:\> "TestPSPipe" | Test-Pipe
#>
function Test-PSPipe
{
	[CmdletBinding()]
	param ([Parameter(Mandatory = $true, ValueFromPipeline = $true)]
		[string]$Name)
	
	process
	{		
		(Get-PSPipes | where Name -eq $Name) -ne $null
	}
}

<#
	.SYNOPSIS
	Creates a new named pipe.

	.DESCRIPTION
	Creates a named pipe and waits for a client to connect.

	.PARAMETER Name
	Name to give to pipe.

	.OUTPUTS
	Custom pipe object that can be passed to other cmdlets in this module.

	.EXAMPLE
	$pipe = new-PSPipe -Name 

#>
function New-PSPipe
{
	[CmdletBinding()]
	param ([Parameter(Mandatory=$true)][string]$Name)
	
	process
	{		
		Write-Verbose "Creating pipe server for pipe named $Name"
		$rd = [PSCustomObject]@{
			Pipe = new-object System.IO.Pipes.NamedPipeServerStream($Name, [System.IO.Pipes.PipeDirection]::InOut)
			Name = $Name
			Reader = $null
			Writer = $null
		}
		
		Write-Verbose "Waiting for client to connect..."
		
		$rd.Pipe.WaitForConnection()
		Write-Verbose "Client connect."
		
		$rd.Reader = new-object System.IO.StreamReader($rd.Pipe)
		$rd.Writer = New-Object System.IO.StreamWriter($rd.Pipe)
		$rd.Writer.AutoFlush = $true
		Write-Verbose "Attached Stream writer and reader"
		
		$rd
		Write-Verbose "Pipe ready!"
	}
}

<#
	.SYNOPSIS
	Writes a PSObject to pipe.

	.DESCRIPTION
	Serialises a PSObject and sends it over a pipe. PSObject will be reconstructed with Read-PSPipe.

	.INPUT
	PSObject to be serialised.

	Note: Usual psobject serialisation requirements apply.

	.PARAMETER Object
	Object to be seriealised.

	.PARAMETER Pipe
	Pipe object to write the object to. This can be created by New-PSPipe or Connect-PSPipe.

	.EXAMPLE
	C:\> "Hi how are you?" | Write-PSPipe -Pipe $pipe

	.EXAMPLE
	C:\> Write-PSPipe -Object (Get-Process) -Pipe $pipe
#>
function Write-PSPipe
{
	[CmdletBinding()]
	param
	(
		[parameter(Mandatory = $true, ValueFromPipeline = $true)]
		$Object,
		[parameter(Mandatory = $true)]
		$Pipe
	)
	
	process
	{		
		$text = $Object | ConvertTo-Base64
		$Pipe.Writer.WriteLine($text)
	}
}

<#
	.SYNOPSIS
	Reads a PSObject from a pipe.

	.DESCRIPTION
	deserialises psobject from a pipe and returns it.

	Note: Usual psobject serialisation requirements apply.

	.PARAMETER Pipe
	The pipe object to retrive the object from. If no pipe object exists in pipe 
	powershell will wait.

	.OUTPUTS
	PSObject deseriealised from pipe.

	.EXAMPLE
	C:\> $data = Read-PSPipe -Pipe $Pipe
#>
function Read-PSPipe
{
	[CmdletBinding()]
	param ([parameter(Mandatory = $true)]
		$Pipe)
	
	process
	{	
		$Pipe.Reader.ReadLine() | ConvertFrom-Base64
	}
	
}

<#
	.SYNOPSIS
	Closes pipe.

	.DESCRIPTION
	Closes a pipe connection. Works on pipes created by either New-PSPipe or Connect-PSPipe
#>
function Disconnect-PSPipe
{
	[CmdletBinding()]
	param ([Parameter(Mandatory = $true, ValueFromPipeline = $true)]
		$Object)
	process
	{
		$Object.Pipe.Dispose()
	}	
}

<#
	.SYNOPSIS
	Connects to an existing pipe.

	.DESCRIPTION
	Connects to an existing pipe created by New-PSPipe.

	Warning: Do not connect to other processes pipes or you will get unexpected results.

	.PARAMETER Name
	Name of the pipe to connect to.

	Note: You don't need to pass in the full path. For pipe \\.\pipes\mypipe you would only 
	need to pass in mypipe.

	.PARAMETER ComputerName
	The name of the computer you want to connect to. If not specified localhost is used.

	.OUTPUTS
	Returns a customobject which can be used with Read-PSPipe and Write-PSPipe to interact with the pipe.

	.EXAMPLE
	C:\> $pipe = Connect-PSPipe -Name MyPipe

	.EXAMPLE
	C:\> $pipe = Connect-PSPipe -Name MyPipe -ComputerName RemoteHost01
#>
function Connect-PSPipe
{
	[CmdletBinding()]
	param ([Parameter(Mandatory = $true)]
		[string]$Name,
		[string]$ComputerName = ".")
	
	process
	{
		$rd = [pscustomobject]@{
			Pipe = new-object System.IO.Pipes.NamedPipeClientStream($ComputerName, $Name, [System.IO.Pipes.PipeDirection]::InOut,
			[System.IO.Pipes.PipeOptions]::None,
			[System.Security.Principal.TokenImpersonationLevel]::Impersonation)
			Name = $PipeName
			Reader = $null
			Writer = $null
		}
		
		$rd.Pipe.Connect()
		$rd.Reader = new-object System.IO.StreamReader($rd.Pipe)
		$rd.Writer = New-Object System.IO.StreamWriter($rd.Pipe)
		$rd.Writer.AutoFlush = $true
		$rd
	}
}

<#
	.SYNOPSIS
	Returns current active pipes on the system.

	.DESCRIPTION
	Returns a list of open pipes on the system. This includes pipes opened by other processes.

	.OUTPUTS
	Custom objects that contain the name of the pipe and its path.

	.EXAMPLE
	C:\> Get-PSPipe
#>
function Get-PSPipe
{
	[CmdletBinding()]
	param ()
	
	process
	{
		$pipes = [System.io.directory]::getfiles("\\.\pipe\")
		
		foreach ($p in $pipes)
		{
			[PSCustomObject]@{
				Name = $p.Replace("\\.\pipe\", "")
				Path = $p
			}
		}
	}
}

<#
	.SYNOPSIS
	Converts a PSObject ot Base64 string.

	.DESCRIPTION
	This function is used internally by the module to convert a psobject to a base64 string.

	.INPUT
	PSObject to serialise.

	.PARAMETER Object
	PSObject to serailise

	.OUTPUTS
	A base64 string that can be deserialised by ConvertFrom-Base64 back into a psobject.

	.EXAMPLE
	C:\> $text = Get-Process | ConvertTo-Base64
#>
function ConvertTo-Base64
{
	[CmdletBinding()]
	param ([Parameter(Mandatory = $true, ValueFromPipeline = $true)]
		$Object)
	
	process
	{
		$data = $object | ConvertTo-Json -Compress
		$bytes = [System.Text.Encoding]::Unicode.GetBytes($data)
		[Convert]::ToBase64String($Bytes)
	}
	
}

<#
	.SYNOPSIS
	Converts Base64 string back into a PSObject.

	.DESCRIPTION
	Converts string created by ConvertTo-Base64 back into its deserialised PSObject.

	.INPUT
	Base64 encoded string of PSObject created by ConvertTo-Base64.

	.PARAMETER Text
	Base64 encoded string of PSObject created by ConvertTo-Base64.

	.OUTPUTS
	PSObject deserialised from base64 string.

	.EXAMPLE
	C:\> $myvar = $text | ConvertFrom-Base64 
#>
function ConvertFrom-Base64
{
	[CmdletBinding()]
	param ([Parameter(Mandatory = $true, ValueFromPipeline = $true)]
		$Text)
	
	process
	{
		$json = [System.Text.Encoding]::Unicode.GetString([System.Convert]::FromBase64String($text))
		$json | ConvertFrom-Json
	}
}

Export-ModuleMember Connect-PSPipe
Export-ModuleMember ConvertFrom-Base64
Export-ModuleMember ConvertTo-Base64
Export-ModuleMember Disconnect-PSPipe
Export-ModuleMember Get-PSPipe
Export-ModuleMember New-PSPipe
Export-ModuleMember Read-PSPipe
Export-ModuleMember Test-Pipe
Export-ModuleMember Write-PSPipe