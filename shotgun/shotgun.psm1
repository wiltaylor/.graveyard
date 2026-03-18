<#
	.SYNOPSIS
	Allows the same block of script to be run multiple times.
	.DESCRIPTION
    This cmdlet lets the user trigger a script block repeatedly by simply pressing enter.

    This is useful for development work flows where you need to run the same commands again and again.

    The cmdlet can run in 2 modes. First mode is where you pass a script block via the Code parameter. This
    will simply just let the user press enter to rerun the script block. The second mode will read a shotgun.psd1 file
    from the current directory and present the user with options specified in that file.

    An example file layout is as follows:

    @{
        Action1 = {
            Write-Host "Action 1 ran"
        }

        Action2 = {
            Write-Host "Action 2 ran"
        }

        Action3 = {
            Write-Host "Action 3 ran"
        }
    }



	.PARAMETER Code
	Code block to run.
#>
function Start-Shotgun 
{
    [CmdletBinding()]
    param([Parameter(Mandatory = $false, ValueFromPipeline = $true)][ScriptBlock]$Code)

    $defaultOption = -1

    while($true) 
    {
        if($code -ne $null) 
        {
            &$Code

            $result = Read-Host "Press enter to rerun commands. Q or Ctrl + C to quit"

            if($result.ToLower() -eq "q") 
            {
                return
            }
        }
        else
        {
            if(-not(Test-Path ".\shotgun.psd1")) 
            {
                Write-Host "Please either use -Code switch or a shotgun.psd1 file"
                return
            }

            $options = Get-Content .\shotgun.psd1 -raw | Invoke-Expression
            $index = 0
            $scripts = @()
            foreach($i in ($options.Keys | Sort-Object -Descending)) 
            {
                Write-Host "[$index] - $i"
                $scripts += $options[$i]
                $index++
            }

            Write-Host "[q] - Quit shotgun"

            $selectedoption = -1

            while($selectedoption -eq -1 -or $selectedoption -ge $index) 
            {
                $inputtext = ""

                if($defaultOption -eq -1) 
                {
                    $inputtext = Read-Host ">"
                } 
                else
                {
                    $inputtext = Read-Host "[$defaultOption]>"
                }
                
                if($inputtext.ToLower() -eq "q") 
                {
                    return
                }                

                if($inputtext -eq "")
                {
                    $selectedoption = [int]::Parse($defaultOption)
                }
                else
                {
                    $selectedoption = [int]::Parse($inputtext)
                }
            }

            $defaultOption = $selectedoption
            &$scripts[$selectedoption]
        }
    }
}

New-Alias -Name shotgun -Value Start-Shotgun