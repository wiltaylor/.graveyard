.  $PSScriptRoot\Save-VMSnapshot.ps1
.  $PSScriptRoot\Fakes\Fake_VM.ps1


Describe "Save-VMSnapshot" {
	Context "Can save snapshot" {
		$script:TestFlag = $false
		$fakevm = New-FakeVM -CreateSnapshot {
			param($Name)

			if($Name -eq "MySnapshot") { $script:TestFlag = $true}
		}
	
		It "Save snapshot" {
			$fakevm | Save-VMSnapshot -Name "MySnapshot"
			$script:TestFlag | should be $true
		}
	}
}