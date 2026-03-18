# Easy NuGet
Simple library that can download, extract and upload nuget packages using v3 feeds.

I created this library instead of using the NuGet client libraries because they don't fully work with .net standard and I found them to be overly complex for what I needed them to do.

# V2 and lower?
No this package only supports V3 and up feeds at the moment. I might add these if there is demand for it or I need it for one of my side projects.

# Installation
To install the package you can search for it in the NuGet Package Manager or install from the package console with: 

```
Install-Package EasyNuGet
```

# Usage
Usage of the library is fairly straight forward.

## Service Locator

First thing you need to do is create a ServiceLocator (not to be confused with the pattern) which is used to read a nuget feed v3 and find end points to perform various
actions.

```
var loc = new ServiceLocator();

// If you want to use the default nuget feed you don't need to do anything.
// However if you want to use your own feed set it here.
loc.NuGetFeedUri = "https://yourserver/v3/index.json";
```

## Package Search

To search for a package you can do the following:

```
var loc = new ServiceLocator();
var search = new PackageSearcher(loc);

var packages = search.Search("EasyNuGet");

//Packages contains all the packages that match the query entered.
//For query documentation see: https://docs.microsoft.com/en-us/nuget/consume-packages/finding-and-choosing-packages#search-syntax
foreach(var p in packages)
    Console.WriteLine($"{p.Id} - {p.Title} - {p.Version} - {string.Join(",", p.Tags)}");

```

## Download and Extract
To download and extract a package do the following:

```
var loc = new ServiceLocator();
var downloader = new PackageDownloader(loc);

downloader.Download("EasyNuGet", "0.1.0", "c:\\myprojectfolder");

//This will download the package into a {packagename}\{Version} folder inside the target folder specified.
```

## Checking if package is installed
You can check if a package is installed at target.

```
var loc = new ServiceLocator();
var downloader = new PackageDownloader(loc);

if(downloader.IsInstalled("EasyNuGet", "0.1.0", "c:\\myprojectfolder"))
    Console.WriteLine("Package Installed");
else
    Console.WriteLine("Package Not Installed");
```

## How to download nuget package file.
If you want to download the nuget package itself and not extract it you can do that by doing the following:

```
var loc = new ServiceLocator();
var downloader = new PackageDownloader(loc);
downloader.DownloadArchive("EasyNuGet", "0.1.0", $"C:\\MyPackages\\EasyNuGet.0.1.0.nupkg");
```

## Uploading a nuget package
To upload a nuget package do the following:

```
var loc = new ServiceLocator();
var uploader = new PackageUploader(loc);

uploader.UploadPackage("D:\\mypacks\\mypack.0.1.0.nupkg", "putyourapikeyhere");
```

# Bugs, Feature Requests
* Please feel free to raise issues on this repository.

# Contributing
To contribute please do the following:

* Raise an Issue on this repository.
* Fork this repository.
* Make your changes.
* Make Unit tests for your changes that pass.
* Make sure other unit tests pass.
* Rebase so your pull request is a single commit.
* Send a pull request and reference your issue.


