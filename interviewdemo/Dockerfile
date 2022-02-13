FROM mcr.microsoft.com/dotnet/sdk:6.0 AS build
WORKDIR /app
RUN apt-get update && apt-get install nodejs npm -y

COPY . .
RUN dotnet restore
WORKDIR /app/InterviewDemoApp.Web
RUN dotnet publish -c Release -o out

FROM mcr.microsoft.com/dotnet/aspnet:6.0
WORKDIR /app
COPY --from=build /app/InterviewDemoApp.Web/out .
ENTRYPOINT ["dotnet", "InterviewDemoApp.Web.dll"]