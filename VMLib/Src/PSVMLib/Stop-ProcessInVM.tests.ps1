.  $PSScriptRoot\Stop-ProcessInVM.ps1
.  $PSScriptRoot\Fakes\Fake_VM.ps1

Describe "Stop-ProcessInVM" {
	Context "Can Stop Process" {
		
		$script:TestFlag = $false				
		$fakevm = new-fakevm -KillProcess {
			param($procid)

			if($procid -eq 1000) { $script:TestFlag = $true}
		}

		$fakeprocess = [PSCustomObject]@{
			Name = "MyProcess.exe"
			ProcessID = 1000
			VM = $fakeVM
		}

		It "Stop Process" {
			$fakeprocess | Stop-ProcessInVM
			$script:TestFlag | should be $true
		}
	}
}