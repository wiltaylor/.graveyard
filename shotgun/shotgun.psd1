@{

# Script module or binary module file associated with this manifest.
RootModule = 'shotgun.psm1'

# Version number of this module.
ModuleVersion = '1.1.0'

# ID used to uniquely identify this module
GUID = '5505a4d4-0a44-42b5-a724-b5245cdddebd'

# Author of this module
Author = 'Wil Taylor'

# Copyright statement for this module
Copyright = 'Copyright (c) 2017 by Wil Taylor.'

# Description of the functionality provided by this module
Description = 'Simple module which allows easy re-run the same commands. Mostly useful for development cycles.'

# Minimum version of the Windows PowerShell engine required by this module
PowerShellVersion = '2.0'

# Functions to export from this module
FunctionsToExport = @( 
    'Start-Shotgun'
)

# # Aliases to export from this module
AliasesToExport = 'shotgun'

PrivateData = @{
    # PSData is module packaging and gallery metadata embedded in PrivateData
    # It's for rebuilding PowerShellGet (and PoshCode) NuGet-style packages
    # We had to do this because it's the only place we're allowed to extend the manifest
    # https://connect.microsoft.com/PowerShell/feedback/details/421837
    PSData = @{
        # The primary categorization of this module (from the TechNet Gallery tech tree).
        Category = "Tool"

        # Keyword tags to help users find this module via navigations and search.
        Tags = @('shotgun')

        # Indicates this is a pre-release/testing version of the module.
        IsPrerelease = 'False'
    }
}

# HelpInfo URI of this module
# HelpInfoURI = ''

# Default prefix for commands exported from this module. Override the default prefix using Import-Module -Prefix.
# DefaultCommandPrefix = ''

}
