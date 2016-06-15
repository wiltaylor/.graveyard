foreach($file in Get-ChildItem -Path $PSScriptRoot -File)
{
	. $file.FullName
}