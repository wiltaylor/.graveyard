.  $PSScriptRoot\New-VMShare.ps1
.  $PSScriptRoot\Fakes\Fake_VM.ps1

Describe "New-VMShare" {
	Context "Can add shared folder." {
		$script:TestFlag = $false
		$FakeVM = New-FakeVM -AddSharedFolder {
			param($Name, $Path, $WriteAccess)

			if($Name -eq "MyShare" -and $Path -eq "c:\myfolder" -and $WriteACcess -eq $false){
				$script:TestFlag = $true
			}

		}
		
		It "Can add shared folder" {
			$FakeVM | New-VMShare -Name "MyShare" -ReadOnly -Path "c:\myfolder"
			$script:TestFlag | should be $true
		}
	}
}