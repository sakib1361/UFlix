using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Data;
using System;


namespace UFlix.Cross.Templates
{

    public class TicksToTimeSpanConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, string language) => new TimeSpan((long)value);
        public object ConvertBack(object value, Type targetType, object parameter, string language) => ((TimeSpan)value).Ticks;
    }

    public class TicksToTimeConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, string language)
        {
            if (value is long ticks) return new TimeSpan(ticks).ToString(@"hh\:mm\:ss");
            else if (value is double tickDouble) return TimeSpan.FromTicks((long)tickDouble).ToString(@"hh\:mm\:ss");
            else return value;
        }
        public object ConvertBack(object value, Type targetType, object parameter, string language) => throw new NotImplementedException();
    }

    public class TicksToSecondsConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, string language) => (long)value / 10000000.0;

        public object ConvertBack(object value, Type targetType, object parameter, string language) => (long)((double)value * 10000000);
    }

    public class TicksToMilliSecondsConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, string language) => (int)((long)value / 10000);

        public object ConvertBack(object value, Type targetType, object parameter, string language) => long.TryParse(value.ToString(), out var res) ? res * 10000 : 0;
    }

    public class SecondsToStringConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, string language)
        {
            if (value is double data)
                return TimeSpan.FromSeconds(data).ToString(@"hh\:mm\:ss");
            return null;
        }

        public object ConvertBack(object value, Type targetType, object parameter, string language)
        {
            throw new NotImplementedException();
        }
    }

    public class TimeSpanToStringConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, string language)
        {
            if (value is TimeSpan data)
                return data.ToString(@"hh\:mm\:ss");
            return null;
        }

        public object ConvertBack(object value, Type targetType, object parameter, string language)
        {
            throw new NotImplementedException();
        }
    }

    public class ParameterVisibiliyConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, string language)
        {
            if (value == null || parameter == null) return Visibility.Collapsed;
            if (value.ToString() == parameter.ToString()) return Visibility.Visible;
            return Visibility.Collapsed;
        }

        public object ConvertBack(object value, Type targetType, object parameter, string language)
        {
            throw new NotImplementedException();
        }
    }

    public class BoolVisibiliyConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, string language)
        {
            if(value is bool boolValue)
                return boolValue? Visibility.Visible: Visibility.Collapsed;
            return Visibility.Collapsed;
        }

        public object ConvertBack(object value, Type targetType, object parameter, string language)
        {
            throw new NotImplementedException();
        }
    }

    public class NullVisibilityConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, string language) => value == null ? Visibility.Collapsed : Visibility.Visible;

        public object ConvertBack(object value, Type targetType, object parameter, string language) => throw new NotImplementedException();
    }

    public class DurationBadgeConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, string language) =>
            value is TimeSpan duration ? YoutubeService.FormatDuration(duration) : null;

        public object ConvertBack(object value, Type targetType, object parameter, string language) => throw new NotImplementedException();
    }
}
