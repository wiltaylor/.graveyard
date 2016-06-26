.  $PSScriptRoot\Copy-FileToVM.ps1
.  $PSScriptRoot\Fakes\Fake_VM.ps1


Describe "Copy-FileToVM" {
	Context "can copy file from host to VM" {
		

			$script:TestFlag = $false
			$FakeVM = New-FakeVM -CopyToVM {	
				param($guestpath, $hostpath) 
				if($hostpath -eq "c:\\hostpath.txt" -and $guestpath -eq "c:\\guestpath.txt") {
					$script:TestFlag = $true
				}
			}
	
		It "Can copy files." {
			$FakeVM | Copy-FileToVM -Source "c:\\guestpath.txt" -Destination "c:\\hostpath.txt"
			$script:TestFlag | should be $true
		}
	}
}