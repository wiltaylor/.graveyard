.  $PSScriptRoot\Invoke-PowershellInVM.ps1
.  $PSScriptRoot\Fakes\Fake_VM.ps1

Describe "Invoke-PowershellInVM" {
	Context "Can execute powershell in VM" {
		$script:TestFlag = $false
		$fakevm = New-FakeVM -ExecutePowershell {
			param($script)

			if($script -eq "<#Poshscript#>") {$script:TestFlag = $true}
		}

		It "Excutes powershell in VM" {
			$fakevm | Invoke-PowershellInVM -Code {<#Poshscript#>} -Username "MyUsername" -Password "MyPassword"
		}

		It "Set the credentials as expected." {
			$fakevm.Username | should be "MyUsername"
			$fakevm.Password | should be "MyPassword"
		}

	}
}