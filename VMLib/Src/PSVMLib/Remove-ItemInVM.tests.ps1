.  $PSScriptRoot\Remove-ItemInVM.ps1
.  $PSScriptRoot\Fakes\Fake_VM.ps1

Describe "Remove-ItemInVM" {
	Context "Can remove items in VM" {
		$script:TestFlagFile = $false
		$script:TestFlagFolder = $false

		$fakeVM = New-FakeVM -DeleteFile {
			param($Path)

			if($path -eq "c:\myfile.txt") { $script:TestFlagFile = $true }
		} -DeleteDirectory {
			param($Path)

			if($path -eq "c:\myfolder") { $script:TestFlagFolder = $true }
		}

		It "Can remove file" {
			$fakeVM | Remove-ItemInVM -Path "c:\myfile.txt" -Type File
			$script:TestFlagFile | should be $true
		}

		It "Can remove folder" {
			$fakeVM | Remove-ItemInVM -Path "c:\myfolder" -Type Directory
			$script:TestFlagFolder | should be $true
		}

	}
}