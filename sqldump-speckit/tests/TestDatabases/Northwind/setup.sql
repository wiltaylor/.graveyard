-- Enhanced Northwind Database Setup for MSSQL Dump Tool Testing
-- Includes tables, views, stored procedures, functions, triggers for comprehensive testing

USE master;
GO

-- Drop database if exists
IF EXISTS (SELECT name FROM sys.databases WHERE name = N'Northwind')
BEGIN
    ALTER DATABASE Northwind SET SINGLE_USER WITH ROLLBACK IMMEDIATE;
    DROP DATABASE Northwind;
END
GO

-- Create Northwind database
CREATE DATABASE Northwind;
GO

USE Northwind;
GO

-- ============================================================================
-- TABLES
-- ============================================================================

-- Categories table
CREATE TABLE dbo.Categories (
    CategoryID INT IDENTITY(1,1) NOT NULL,
    CategoryName NVARCHAR(15) NOT NULL,
    Description NVARCHAR(MAX) NULL,
    Picture VARBINARY(MAX) NULL,
    CONSTRAINT PK_Categories PRIMARY KEY CLUSTERED (CategoryID)
);
GO

-- Suppliers table
CREATE TABLE dbo.Suppliers (
    SupplierID INT IDENTITY(1,1) NOT NULL,
    CompanyName NVARCHAR(40) NOT NULL,
    ContactName NVARCHAR(30) NULL,
    ContactTitle NVARCHAR(30) NULL,
    Address NVARCHAR(60) NULL,
    City NVARCHAR(15) NULL,
    Region NVARCHAR(15) NULL,
    PostalCode NVARCHAR(10) NULL,
    Country NVARCHAR(15) NULL,
    Phone NVARCHAR(24) NULL,
    Fax NVARCHAR(24) NULL,
    HomePage NVARCHAR(MAX) NULL,
    CONSTRAINT PK_Suppliers PRIMARY KEY CLUSTERED (SupplierID)
);
GO

-- Products table (with FK references)
CREATE TABLE dbo.Products (
    ProductID INT IDENTITY(1,1) NOT NULL,
    ProductName NVARCHAR(40) NOT NULL,
    SupplierID INT NULL,
    CategoryID INT NULL,
    QuantityPerUnit NVARCHAR(20) NULL,
    UnitPrice MONEY NULL CONSTRAINT DF_Products_UnitPrice DEFAULT (0),
    UnitsInStock SMALLINT NULL CONSTRAINT DF_Products_UnitsInStock DEFAULT (0),
    UnitsOnOrder SMALLINT NULL CONSTRAINT DF_Products_UnitsOnOrder DEFAULT (0),
    ReorderLevel SMALLINT NULL CONSTRAINT DF_Products_ReorderLevel DEFAULT (0),
    Discontinued BIT NOT NULL CONSTRAINT DF_Products_Discontinued DEFAULT (0),
    CONSTRAINT PK_Products PRIMARY KEY CLUSTERED (ProductID),
    CONSTRAINT FK_Products_Categories FOREIGN KEY (CategoryID) 
        REFERENCES dbo.Categories (CategoryID),
    CONSTRAINT FK_Products_Suppliers FOREIGN KEY (SupplierID) 
        REFERENCES dbo.Suppliers (SupplierID),
    CONSTRAINT CK_Products_UnitPrice CHECK (UnitPrice >= 0),
    CONSTRAINT CK_UnitsInStock CHECK (UnitsInStock >= 0),
    CONSTRAINT CK_UnitsOnOrder CHECK (UnitsOnOrder >= 0),
    CONSTRAINT CK_ReorderLevel CHECK (ReorderLevel >= 0)
);
GO

-- Customers table
CREATE TABLE dbo.Customers (
    CustomerID NCHAR(5) NOT NULL,
    CompanyName NVARCHAR(40) NOT NULL,
    ContactName NVARCHAR(30) NULL,
    ContactTitle NVARCHAR(30) NULL,
    Address NVARCHAR(60) NULL,
    City NVARCHAR(15) NULL,
    Region NVARCHAR(15) NULL,
    PostalCode NVARCHAR(10) NULL,
    Country NVARCHAR(15) NULL,
    Phone NVARCHAR(24) NULL,
    Fax NVARCHAR(24) NULL,
    CONSTRAINT PK_Customers PRIMARY KEY CLUSTERED (CustomerID)
);
GO

-- Employees table
CREATE TABLE dbo.Employees (
    EmployeeID INT IDENTITY(1,1) NOT NULL,
    LastName NVARCHAR(20) NOT NULL,
    FirstName NVARCHAR(10) NOT NULL,
    Title NVARCHAR(30) NULL,
    TitleOfCourtesy NVARCHAR(25) NULL,
    BirthDate DATETIME NULL,
    HireDate DATETIME NULL,
    Address NVARCHAR(60) NULL,
    City NVARCHAR(15) NULL,
    Region NVARCHAR(15) NULL,
    PostalCode NVARCHAR(10) NULL,
    Country NVARCHAR(15) NULL,
    HomePhone NVARCHAR(24) NULL,
    Extension NVARCHAR(4) NULL,
    Photo VARBINARY(MAX) NULL,
    Notes NVARCHAR(MAX) NULL,
    ReportsTo INT NULL,
    PhotoPath NVARCHAR(255) NULL,
    CONSTRAINT PK_Employees PRIMARY KEY CLUSTERED (EmployeeID),
    CONSTRAINT FK_Employees_Employees FOREIGN KEY (ReportsTo) 
        REFERENCES dbo.Employees (EmployeeID)
);
GO

-- Shippers table
CREATE TABLE dbo.Shippers (
    ShipperID INT IDENTITY(1,1) NOT NULL,
    CompanyName NVARCHAR(40) NOT NULL,
    Phone NVARCHAR(24) NULL,
    CONSTRAINT PK_Shippers PRIMARY KEY CLUSTERED (ShipperID)
);
GO

-- Orders table
CREATE TABLE dbo.Orders (
    OrderID INT IDENTITY(1,1) NOT NULL,
    CustomerID NCHAR(5) NULL,
    EmployeeID INT NULL,
    OrderDate DATETIME NULL,
    RequiredDate DATETIME NULL,
    ShippedDate DATETIME NULL,
    ShipVia INT NULL,
    Freight MONEY NULL CONSTRAINT DF_Orders_Freight DEFAULT (0),
    ShipName NVARCHAR(40) NULL,
    ShipAddress NVARCHAR(60) NULL,
    ShipCity NVARCHAR(15) NULL,
    ShipRegion NVARCHAR(15) NULL,
    ShipPostalCode NVARCHAR(10) NULL,
    ShipCountry NVARCHAR(15) NULL,
    CONSTRAINT PK_Orders PRIMARY KEY CLUSTERED (OrderID),
    CONSTRAINT FK_Orders_Customers FOREIGN KEY (CustomerID) 
        REFERENCES dbo.Customers (CustomerID),
    CONSTRAINT FK_Orders_Employees FOREIGN KEY (EmployeeID) 
        REFERENCES dbo.Employees (EmployeeID),
    CONSTRAINT FK_Orders_Shippers FOREIGN KEY (ShipVia) 
        REFERENCES dbo.Shippers (ShipperID)
);
GO

-- Order Details table (composite key)
CREATE TABLE dbo.[Order Details] (
    OrderID INT NOT NULL,
    ProductID INT NOT NULL,
    UnitPrice MONEY NOT NULL CONSTRAINT DF_Order_Details_UnitPrice DEFAULT (0),
    Quantity SMALLINT NOT NULL CONSTRAINT DF_Order_Details_Quantity DEFAULT (1),
    Discount REAL NOT NULL CONSTRAINT DF_Order_Details_Discount DEFAULT (0),
    CONSTRAINT PK_Order_Details PRIMARY KEY CLUSTERED (OrderID, ProductID),
    CONSTRAINT FK_Order_Details_Orders FOREIGN KEY (OrderID) 
        REFERENCES dbo.Orders (OrderID),
    CONSTRAINT FK_Order_Details_Products FOREIGN KEY (ProductID) 
        REFERENCES dbo.Products (ProductID),
    CONSTRAINT CK_Quantity CHECK (Quantity > 0),
    CONSTRAINT CK_Discount CHECK (Discount >= 0 AND Discount <= 1),
    CONSTRAINT CK_UnitPrice CHECK (UnitPrice >= 0)
);
GO

-- ============================================================================
-- INDEXES
-- ============================================================================

CREATE INDEX IX_Products_CategoryID ON dbo.Products(CategoryID);
CREATE INDEX IX_Products_SupplierID ON dbo.Products(SupplierID);
CREATE INDEX IX_Orders_CustomerID ON dbo.Orders(CustomerID);
CREATE INDEX IX_Orders_EmployeeID ON dbo.Orders(EmployeeID);
CREATE INDEX IX_Orders_ShipVia ON dbo.Orders(ShipVia);
CREATE INDEX IX_Orders_OrderDate ON dbo.Orders(OrderDate);
CREATE INDEX IX_Customers_City ON dbo.Customers(City);
CREATE INDEX IX_Customers_CompanyName ON dbo.Customers(CompanyName);
GO

-- ============================================================================
-- SAMPLE DATA
-- ============================================================================

-- Insert Categories
SET IDENTITY_INSERT dbo.Categories ON;
INSERT INTO dbo.Categories (CategoryID, CategoryName, Description) VALUES
(1, 'Beverages', 'Soft drinks, coffees, teas, beers, and ales'),
(2, 'Condiments', 'Sweet and savory sauces, relishes, spreads, and seasonings'),
(3, 'Confections', 'Desserts, candies, and sweet breads'),
(4, 'Dairy Products', 'Cheeses'),
(5, 'Grains/Cereals', 'Breads, crackers, pasta, and cereal'),
(6, 'Meat/Poultry', 'Prepared meats'),
(7, 'Produce', 'Dried fruit and bean curd'),
(8, 'Seafood', 'Seaweed and fish');
SET IDENTITY_INSERT dbo.Categories OFF;
GO

-- Insert Suppliers (sample)
SET IDENTITY_INSERT dbo.Suppliers ON;
INSERT INTO dbo.Suppliers (SupplierID, CompanyName, ContactName, City, Country, Phone) VALUES
(1, 'Exotic Liquids', 'Charlotte Cooper', 'London', 'UK', '(171) 555-2222'),
(2, 'New Orleans Cajun Delights', 'Shelley Burke', 'New Orleans', 'USA', '(100) 555-4822'),
(3, 'Grandma Kelly''s Homestead', 'Regina Murphy', 'Ann Arbor', 'USA', '(313) 555-5735');
SET IDENTITY_INSERT dbo.Suppliers OFF;
GO

-- Insert Products
SET IDENTITY_INSERT dbo.Products ON;
INSERT INTO dbo.Products (ProductID, ProductName, SupplierID, CategoryID, UnitPrice, UnitsInStock, Discontinued) VALUES
(1, 'Chai', 1, 1, 18.00, 39, 0),
(2, 'Chang', 1, 1, 19.00, 17, 0),
(3, 'Aniseed Syrup', 1, 2, 10.00, 13, 0),
(4, 'Chef Anton''s Cajun Seasoning', 2, 2, 22.00, 53, 0),
(5, 'Chef Anton''s Gumbo Mix', 2, 2, 21.35, 0, 1),
(6, 'Grandma''s Boysenberry Spread', 3, 2, 25.00, 120, 0);
SET IDENTITY_INSERT dbo.Products OFF;
GO

-- Insert Customers (sample)
INSERT INTO dbo.Customers (CustomerID, CompanyName, ContactName, City, Country) VALUES
('ALFKI', 'Alfreds Futterkiste', 'Maria Anders', 'Berlin', 'Germany'),
('ANATR', 'Ana Trujillo Emparedados y helados', 'Ana Trujillo', 'México D.F.', 'Mexico'),
('ANTON', 'Antonio Moreno Taquería', 'Antonio Moreno', 'México D.F.', 'Mexico'),
('AROUT', 'Around the Horn', 'Thomas Hardy', 'London', 'UK'),
('BERGS', 'Berglunds snabbköp', 'Christina Berglund', 'Luleå', 'Sweden');
GO

-- Insert Employees
SET IDENTITY_INSERT dbo.Employees ON;
INSERT INTO dbo.Employees (EmployeeID, LastName, FirstName, Title, HireDate, City, Country, ReportsTo) VALUES
(1, 'Davolio', 'Nancy', 'Sales Representative', '1992-05-01', 'Seattle', 'USA', NULL),
(2, 'Fuller', 'Andrew', 'Vice President, Sales', '1992-08-14', 'Tacoma', 'USA', NULL),
(3, 'Leverling', 'Janet', 'Sales Representative', '1992-04-01', 'Kirkland', 'USA', 2);
SET IDENTITY_INSERT dbo.Employees OFF;
GO

-- Insert Shippers
SET IDENTITY_INSERT dbo.Shippers ON;
INSERT INTO dbo.Shippers (ShipperID, CompanyName, Phone) VALUES
(1, 'Speedy Express', '(503) 555-9831'),
(2, 'United Package', '(503) 555-3199'),
(3, 'Federal Shipping', '(503) 555-9931');
SET IDENTITY_INSERT dbo.Shippers OFF;
GO

-- Insert Orders
SET IDENTITY_INSERT dbo.Orders ON;
INSERT INTO dbo.Orders (OrderID, CustomerID, EmployeeID, OrderDate, RequiredDate, ShipVia, Freight, ShipCity, ShipCountry) VALUES
(10248, 'ALFKI', 1, '1996-07-04', '1996-08-01', 3, 32.38, 'Reims', 'France'),
(10249, 'ANATR', 2, '1996-07-05', '1996-08-16', 1, 11.61, 'Münster', 'Germany'),
(10250, 'ANTON', 3, '1996-07-08', '1996-08-05', 2, 65.83, 'Rio de Janeiro', 'Brazil');
SET IDENTITY_INSERT dbo.Orders OFF;
GO

-- Insert Order Details
INSERT INTO dbo.[Order Details] (OrderID, ProductID, UnitPrice, Quantity, Discount) VALUES
(10248, 1, 18.00, 12, 0),
(10248, 2, 19.00, 10, 0),
(10249, 3, 10.00, 5, 0),
(10250, 4, 22.00, 10, 0.15),
(10250, 6, 25.00, 25, 0.15);
GO

-- ============================================================================
-- VIEWS
-- ============================================================================

-- View: Product sales summary
CREATE VIEW dbo.ProductSales AS
SELECT 
    p.ProductID,
    p.ProductName,
    c.CategoryName,
    SUM(od.Quantity) AS TotalQuantitySold,
    SUM(od.UnitPrice * od.Quantity * (1 - od.Discount)) AS TotalRevenue
FROM dbo.Products p
INNER JOIN dbo.Categories c ON p.CategoryID = c.CategoryID
LEFT JOIN dbo.[Order Details] od ON p.ProductID = od.ProductID
GROUP BY p.ProductID, p.ProductName, c.CategoryName;
GO

-- View: Customer orders summary
CREATE VIEW dbo.CustomerOrders AS
SELECT 
    c.CustomerID,
    c.CompanyName,
    c.ContactName,
    COUNT(o.OrderID) AS TotalOrders,
    SUM(o.Freight) AS TotalFreight
FROM dbo.Customers c
LEFT JOIN dbo.Orders o ON c.CustomerID = o.CustomerID
GROUP BY c.CustomerID, c.CompanyName, c.ContactName;
GO

-- ============================================================================
-- STORED PROCEDURES
-- ============================================================================

-- Procedure: Get products by category
CREATE PROCEDURE dbo.GetProductsByCategory
    @CategoryID INT
AS
BEGIN
    SET NOCOUNT ON;
    
    SELECT 
        ProductID,
        ProductName,
        UnitPrice,
        UnitsInStock,
        Discontinued
    FROM dbo.Products
    WHERE CategoryID = @CategoryID
    ORDER BY ProductName;
END;
GO

-- Procedure: Add new order
CREATE PROCEDURE dbo.AddOrder
    @CustomerID NCHAR(5),
    @EmployeeID INT,
    @OrderDate DATETIME,
    @ShipVia INT,
    @Freight MONEY,
    @OrderID INT OUTPUT
AS
BEGIN
    SET NOCOUNT ON;
    
    INSERT INTO dbo.Orders (CustomerID, EmployeeID, OrderDate, ShipVia, Freight)
    VALUES (@CustomerID, @EmployeeID, @OrderDate, @ShipVia, @Freight);
    
    SET @OrderID = SCOPE_IDENTITY();
END;
GO

-- ============================================================================
-- FUNCTIONS
-- ============================================================================

-- Scalar function: Calculate order total
CREATE FUNCTION dbo.CalculateOrderTotal(@OrderID INT)
RETURNS MONEY
AS
BEGIN
    DECLARE @Total MONEY;
    
    SELECT @Total = SUM(UnitPrice * Quantity * (1 - Discount))
    FROM dbo.[Order Details]
    WHERE OrderID = @OrderID;
    
    RETURN ISNULL(@Total, 0);
END;
GO

-- Table-valued function: Get employee orders
CREATE FUNCTION dbo.GetEmployeeOrders(@EmployeeID INT)
RETURNS TABLE
AS
RETURN
(
    SELECT 
        o.OrderID,
        o.CustomerID,
        o.OrderDate,
        o.Freight,
        dbo.CalculateOrderTotal(o.OrderID) AS OrderTotal
    FROM dbo.Orders o
    WHERE o.EmployeeID = @EmployeeID
);
GO

-- ============================================================================
-- TRIGGERS
-- ============================================================================

-- Trigger: Audit product changes
CREATE TABLE dbo.ProductAudit (
    AuditID INT IDENTITY(1,1) PRIMARY KEY,
    ProductID INT NOT NULL,
    Action VARCHAR(10) NOT NULL,
    ChangedBy NVARCHAR(128) NOT NULL,
    ChangedAt DATETIME NOT NULL DEFAULT GETDATE(),
    OldUnitPrice MONEY NULL,
    NewUnitPrice MONEY NULL
);
GO

CREATE TRIGGER dbo.TR_Products_Update
ON dbo.Products
AFTER UPDATE
AS
BEGIN
    SET NOCOUNT ON;
    
    IF UPDATE(UnitPrice)
    BEGIN
        INSERT INTO dbo.ProductAudit (ProductID, Action, ChangedBy, OldUnitPrice, NewUnitPrice)
        SELECT 
            i.ProductID,
            'UPDATE',
            SYSTEM_USER,
            d.UnitPrice,
            i.UnitPrice
        FROM inserted i
        INNER JOIN deleted d ON i.ProductID = d.ProductID
        WHERE i.UnitPrice <> d.UnitPrice;
    END
END;
GO

PRINT 'Enhanced Northwind database created successfully with tables, views, procedures, functions, and triggers!';
GO
