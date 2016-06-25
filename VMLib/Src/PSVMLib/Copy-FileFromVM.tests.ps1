.  $PSScriptRoot\Copy-FileFromVM.ps1
.  $PSScriptRoot\Fakes\Fake_VM.ps1

Describe "Copy-FileFromVM" {
	Context "Can copy file from VM to local location" {
		
		$script:TestFlag = $false
		$FakeVM = New-FakeVM -CopyFromVM {	
			param($hostpath, $guestpath) 

			if($hostpath -eq "c:\\hostpath.txt" -and $guestpath -eq "c:\\guestpath.txt") {
				$script:TestFlag = $true
			}
			}
	
		It "Can Copy files" {
			$FakeVM | Copy-FileFromVM -Source "c:\\guestpath.txt" -Destination "c:\\hostpath.txt"
			$script:TestFlag | should be $true
		}
	}
}